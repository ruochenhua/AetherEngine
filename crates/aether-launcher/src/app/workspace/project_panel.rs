use crate::project_index::{
    scan_project, IndexSnapshot, IndexStatus, PathHistory, ProjectIndex, ProjectQuery, ProjectSort,
    ResourceKind, ResourceValidity,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

pub(super) struct ProjectPanel {
    root: PathBuf,
    index: ProjectIndex,
    search: String,
    kind: Option<ResourceKind>,
    sort: ProjectSort,
    history: PathHistory,
    selected_resource: Option<String>,
    scan_result: Option<Receiver<(u64, Result<IndexSnapshot, crate::project_index::IndexError>)>>,
}

impl ProjectPanel {
    pub(super) fn new(root: PathBuf) -> Self {
        let mut panel = Self {
            root,
            index: ProjectIndex::default(),
            search: String::new(),
            kind: None,
            sort: ProjectSort::Name,
            history: PathHistory::default(),
            selected_resource: None,
            scan_result: None,
        };
        panel.history.visit("");
        panel.schedule_refresh();
        panel
    }

    pub(super) fn select_resource(&mut self, path: Option<PathBuf>) {
        self.selected_resource = path.map(|path| {
            let path = path.strip_prefix(&self.root).unwrap_or(path.as_path());
            let normalized = normalize_path(path);
            let parent = Path::new(&normalized)
                .parent()
                .and_then(Path::to_str)
                .unwrap_or("");
            self.visit_directory(parent);
            normalized
        });
    }

    pub(super) fn selected_resource(&self) -> Option<&str> {
        self.selected_resource.as_deref()
    }

    fn schedule_refresh(&mut self) {
        let generation = self.index.begin_scan();
        let root = self.root.clone();
        let (sender, receiver) = mpsc::channel();
        self.scan_result = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send((generation, scan_project(&root)));
        });
    }

    fn poll_refresh(&mut self) {
        let Some(receiver) = self.scan_result.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok((generation, result)) => {
                self.index.publish(generation, result);
                self.scan_result = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan_result = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    #[cfg(test)]
    pub(super) fn wait_for_refresh(&mut self) {
        let Some(receiver) = self.scan_result.take() else {
            return;
        };
        if let Ok((generation, result)) = receiver.recv_timeout(std::time::Duration::from_secs(5)) {
            self.index.publish(generation, result);
        }
    }

    pub(super) fn render(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        self.poll_refresh();
        let mut pending_open = None;
        ui.horizontal(|ui| {
            ui.heading("Project resources");
            if ui.button("Refresh").clicked() {
                self.schedule_refresh();
            }
            if ui.button("Close resource details").clicked() {
                self.selected_resource = None;
            }
        });
        self.render_status(ui);

        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.history.back();
            }
            if ui.button("Forward →").clicked() {
                self.history.forward();
            }
            self.render_breadcrumb(ui);
        });

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search resources")
                    .desired_width(190.0),
            );
            egui::ComboBox::from_label("Kind")
                .selected_text(self.kind.map(kind_name).unwrap_or("All kinds"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.kind, None, "All kinds");
                    for kind in [
                        ResourceKind::Scene,
                        ResourceKind::Prefab,
                        ResourceKind::Material,
                        ResourceKind::Gltf,
                        ResourceKind::Obj,
                        ResourceKind::Image,
                        ResourceKind::Unknown,
                    ] {
                        ui.selectable_value(&mut self.kind, Some(kind), kind_name(kind));
                    }
                });
            egui::ComboBox::from_label("Sort by")
                .selected_text(sort_name(self.sort))
                .show_ui(ui, |ui| {
                    for sort in [ProjectSort::Name, ProjectSort::Path, ProjectSort::Kind] {
                        ui.selectable_value(&mut self.sort, sort, sort_name(sort));
                    }
                });
        });

        self.render_details(ui, self.index.snapshot());

        let snapshot = self.index.snapshot().clone();
        let current = self.current_directory().to_owned();
        ui.columns(2, |columns| {
            columns[0].heading("Folders");
            egui::ScrollArea::vertical()
                .id_salt("project_folders")
                .show(&mut columns[0], |ui| {
                    self.render_directory_tree(ui, &snapshot, "", 0);
                });

            columns[1].heading("Resources");
            let query = ProjectQuery::default().search(&self.search).sort(self.sort);
            let resources = query.apply(&snapshot);
            let resources = resources
                .into_iter()
                .filter(|entry| entry.path.starts_with(&current_prefix(&current)))
                .filter(|entry| self.kind.map_or(true, |kind| entry.kind == kind))
                .collect::<Vec<_>>();
            if resources.is_empty() {
                if self.search.trim().is_empty() {
                    ui_state_empty(&mut columns[1], &snapshot, &current);
                } else {
                    columns[1].label("No results");
                }
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("project_resources")
                    .show(&mut columns[1], |ui| {
                        for entry in resources {
                            let selected =
                                self.selected_resource.as_deref() == Some(entry.path.as_str());
                            let label =
                                format!("{}  ·  {}", file_name(&entry.path), kind_name(entry.kind));
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                self.selected_resource = Some(entry.path.clone());
                            }
                            if response.double_clicked()
                                && entry.kind == ResourceKind::Scene
                                && entry.validity == ResourceValidity::Valid
                            {
                                pending_open =
                                    Some(self.root.join(
                                        entry.path.replace('/', std::path::MAIN_SEPARATOR_STR),
                                    ));
                            }
                        }
                    });
            }
        });
        pending_open
    }

    fn render_status(&mut self, ui: &mut egui::Ui) {
        match self.index.status() {
            IndexStatus::Idle => {
                ui.small("Project index is idle");
            }
            IndexStatus::Scanning => {
                ui.small("Scanning project…");
            }
            IndexStatus::Ready => {
                ui.small("Project index ready");
            }
            IndexStatus::Error(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Project scan error: {error}"),
                );
            }
        }
    }

    fn render_directory_tree(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &IndexSnapshot,
        parent: &str,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        let children = child_directories(snapshot, parent);
        if parent.is_empty() && children.is_empty() {
            ui.label("No project folders");
        }
        for child in children {
            let label = match child.as_str() {
                "assets" => "Assets".to_owned(),
                "scenes" => "Scenes".to_owned(),
                _ => file_name(&child).to_owned(),
            };
            let opened = child == self.current_directory()
                || self.current_directory().starts_with(&format!("{child}/"));
            let response = egui::CollapsingHeader::new(label)
                .id_salt(("project_tree", child.clone()))
                .default_open(opened)
                .show(ui, |ui| {
                    self.render_directory_tree(ui, snapshot, &child, depth + 1)
                });
            if response.header_response.clicked() {
                self.visit_directory(&child);
            }
        }
    }

    fn render_breadcrumb(&mut self, ui: &mut egui::Ui) {
        let directory = self.current_directory().to_owned();
        if ui
            .selectable_label(directory.is_empty(), "Project")
            .clicked()
        {
            self.visit_directory("");
        }
        let mut path = String::new();
        for segment in directory.split('/').filter(|part| !part.is_empty()) {
            path = if path.is_empty() {
                segment.to_owned()
            } else {
                format!("{path}/{segment}")
            };
            ui.label("›");
            if ui.selectable_label(path == directory, segment).clicked() {
                self.visit_directory(&path);
            }
        }
    }

    fn render_details(&self, ui: &mut egui::Ui, snapshot: &IndexSnapshot) {
        let Some(path) = self.selected_resource.as_deref() else {
            return;
        };
        let Some(entry) = snapshot.entries.iter().find(|entry| entry.path == path) else {
            ui.separator();
            ui.label("Resource details");
            ui.label(path);
            ui.label("Status: Missing from current index");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Resource details");
            ui.label(format!("Path: {}", entry.path));
            ui.label(format!("Type: {}", kind_name(entry.kind)));
            ui.label(format!("Size: {} bytes", entry.size));
            ui.label(format!("Status: {}", validity_name(entry.validity)));
        });
    }

    fn visit_directory(&mut self, path: &str) {
        if self.history.current() != Some(path) {
            self.history.visit(path);
        }
    }

    pub(super) fn current_directory(&self) -> &str {
        self.history.current().unwrap_or("")
    }
}

fn ui_state_empty(ui: &mut egui::Ui, snapshot: &IndexSnapshot, directory: &str) {
    if !snapshot.missing_roots.is_empty() && directory.is_empty() {
        ui.label(format!(
            "Empty project · missing {}",
            snapshot.missing_roots.join(", ")
        ));
    } else {
        ui.label("No resources in this folder");
    }
}

fn current_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn child_directories(snapshot: &IndexSnapshot, parent: &str) -> Vec<String> {
    let prefix = current_prefix(parent);
    let mut children = snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let rest = entry.path.strip_prefix(&prefix)?;
            let segment = rest.split('/').next()?;
            (rest.contains('/')).then(|| {
                if prefix.is_empty() {
                    segment.to_owned()
                } else {
                    format!("{prefix}{segment}")
                }
            })
        })
        .collect::<Vec<_>>();
    if parent.is_empty() {
        for root in ["assets", "scenes"] {
            children.push(root.to_owned());
        }
    }
    children.sort();
    children.dedup();
    children
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn kind_name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Scene => "Scene",
        ResourceKind::Prefab => "Prefab",
        ResourceKind::Material => "Material",
        ResourceKind::Gltf => "glTF",
        ResourceKind::Obj => "OBJ",
        ResourceKind::Image => "Image",
        ResourceKind::Unknown => "Unknown",
    }
}
fn validity_name(validity: ResourceValidity) -> &'static str {
    match validity {
        ResourceValidity::Valid => "Valid",
        ResourceValidity::Invalid => "Invalid",
        ResourceValidity::Unknown => "Unknown",
    }
}
fn sort_name(sort: ProjectSort) -> &'static str {
    match sort {
        ProjectSort::Name => "Name",
        ProjectSort::Path => "Path",
        ProjectSort::Kind => "Kind",
    }
}
