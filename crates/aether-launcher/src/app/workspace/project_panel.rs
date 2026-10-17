use crate::project_index::{
    scan_project, IndexSnapshot, IndexStatus, PathHistory, ProjectIndex, ProjectQuery, ProjectSort,
    ResourceKind, ResourceValidity,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

mod helpers;
mod preview;
use helpers::{
    child_directories, current_prefix, file_name, kind_name, normalize_path, resource_icon,
    sort_name, ui_state_empty, validity_name,
};

pub(super) struct ProjectPanel {
    root: PathBuf,
    index: ProjectIndex,
    search: String,
    kind: Option<ResourceKind>,
    sort: ProjectSort,
    history: PathHistory,
    selected_resource: Option<String>,
    new_folder_name: String,
    creating_folder: bool,
    scan_result: Option<Receiver<(u64, Result<IndexSnapshot, crate::project_index::IndexError>)>>,
    preview: preview::ResourcePreview,
}

impl ProjectPanel {
    pub(super) fn new(root: PathBuf) -> Self {
        let mut panel = Self {
            preview: preview::ResourcePreview::new(root.clone()),
            root,
            index: ProjectIndex::default(),
            search: String::new(),
            kind: None,
            sort: ProjectSort::Name,
            history: PathHistory::default(),
            selected_resource: None,
            new_folder_name: String::new(),
            creating_folder: false,
            scan_result: None,
        };
        panel.history.visit("");
        panel.schedule_refresh();
        panel
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
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
        self.preview.invalidate();
        let generation = self.index.begin_scan();
        let root = self.root.clone();
        let (sender, receiver) = mpsc::channel();
        self.scan_result = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send((generation, scan_project(&root)));
        });
    }

    pub(super) fn refresh(&mut self) {
        self.schedule_refresh();
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

    pub(super) fn render(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (Option<PathBuf>, Option<super::ProjectFileAction>) {
        self.poll_refresh();
        self.preview.poll();
        let mut pending_open = None;
        let mut project_file = None;
        ui.horizontal(|ui| {
            ui.heading("Project resources");
            ui.add_space(8.0);
            if ui.button("Refresh").clicked() {
                self.schedule_refresh();
            }
            let destination = self.operation_destination();
            ui.label(format!(
                "Destination: {}",
                if destination.is_empty() {
                    "."
                } else {
                    &destination
                }
            ));
            if ui.button("New Folder").clicked() {
                self.creating_folder = true;
                self.new_folder_name.clear();
            }
            if ui
                .add_enabled(
                    destination == "assets" || destination.starts_with("assets/"),
                    egui::Button::new("Import Asset…"),
                )
                .clicked()
            {
                project_file = Some(super::ProjectFileAction::ImportAsset { destination });
            }
            if self.selected_resource.is_some() && ui.button("Close resource details").clicked() {
                self.selected_resource = None;
            }
        });
        if self.creating_folder {
            ui.horizontal(|ui| {
                ui.label("Folder name:");
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.new_folder_name)
                        .hint_text("New folder")
                        .desired_width(180.0),
                );
                let enter =
                    input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
                if ui.button("Create").clicked() || enter {
                    let name = self.new_folder_name.trim().to_owned();
                    if !name.is_empty() {
                        project_file = Some(super::ProjectFileAction::CreateFolder {
                            destination: self.operation_destination(),
                            name,
                        });
                    }
                    self.creating_folder = false;
                }
                if ui.button("Cancel").clicked() || cancel {
                    self.creating_folder = false;
                }
            });
        }
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
                    .desired_width(260.0),
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

        let snapshot = self.index.snapshot().clone();

        let current = self.current_directory().to_owned();
        let browser_size = ui.available_size_before_wrap();
        ui.allocate_ui_with_layout(
            browser_size,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                egui::Panel::left("project_folder_panel")
                    .default_size(260.0)
                    .size_range(140.0..=480.0)
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        ui.heading("Folders");
                        egui::ScrollArea::vertical()
                            .id_salt("project_folders")
                            .show(ui, |ui| {
                                self.render_directory_tree(ui, &snapshot, "", 0);
                            });
                        ui.take_available_space();
                    });

                // egui's native panel handle is registered as the folder
                // panel closes. Keep later resource widgets outside its grab
                // radius so the Resources scroll area cannot steal the drag.
                ui.add_space(12.0);
                let resource_height = browser_size.y.max(80.0);
                let selected = self.selected_resource.is_some();
                let available_width = ui.available_width();
                let resource_width = if selected {
                    (available_width * 0.43)
                        .clamp(280.0, 440.0)
                        .min((available_width - 220.0).max(0.0))
                } else {
                    available_width
                };
                ui.allocate_ui_with_layout(
                    egui::vec2(resource_width, resource_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.heading("Resources");
                        let query = ProjectQuery::default().search(&self.search).sort(self.sort);
                        let resources = query.apply(&snapshot);
                        let resources = resources
                            .into_iter()
                            .filter(|entry| entry.path.starts_with(&current_prefix(&current)))
                            .filter(|entry| self.kind.map_or(true, |kind| entry.kind == kind))
                            .collect::<Vec<_>>();
                        if resources.is_empty() {
                            if self.search.trim().is_empty() {
                                ui_state_empty(ui, &snapshot, &current);
                            } else {
                                ui.label("No results");
                            }
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("project_resources")
                                .show(ui, |ui| {
                                    for entry in resources {
                                        let selected = self.selected_resource.as_deref()
                                            == Some(entry.path.as_str());
                                        let (icon, color) = resource_icon(entry.kind, ui);
                                        let label = format!(
                                            "{}  ·  {}",
                                            file_name(&entry.path),
                                            kind_name(entry.kind)
                                        );
                                        let response = ui
                                            .push_id(&entry.path, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.colored_label(color, icon);
                                                    ui.selectable_label(selected, label)
                                                        .interact(egui::Sense::click_and_drag())
                                                })
                                            })
                                            .inner
                                            .inner;
                                        let response = response.on_hover_text(format!(
                                            "{} · {} · {}",
                                            entry.path,
                                            kind_name(entry.kind),
                                            validity_name(entry.validity)
                                        ));
                                        if matches!(
                                            entry.kind,
                                            ResourceKind::Obj
                                                | ResourceKind::Material
                                                | ResourceKind::Image
                                        ) && entry.validity == ResourceValidity::Valid
                                        {
                                            response.dnd_set_drag_payload(
                                                super::AssetDragPayload {
                                                    relative_path: entry.path.clone(),
                                                    kind: entry.kind,
                                                },
                                            );
                                        }
                                        if response.clicked() {
                                            self.selected_resource = Some(entry.path.clone());
                                        }
                                        if response.double_clicked()
                                            && entry.kind == ResourceKind::Scene
                                            && entry.validity == ResourceValidity::Valid
                                        {
                                            pending_open =
                                                Some(self.root.join(
                                                    entry.path.replace(
                                                        '/',
                                                        std::path::MAIN_SEPARATOR_STR,
                                                    ),
                                                ));
                                        }
                                    }
                                });
                        }
                    },
                );
                if selected {
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), resource_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.render_details(ui, &snapshot),
                    );
                } else {
                    self.preview.select(None);
                }
            },
        );
        (pending_open, project_file)
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

    fn render_details(&mut self, ui: &mut egui::Ui, snapshot: &IndexSnapshot) {
        let Some(path) = self.selected_resource.as_deref() else {
            self.preview.select(None);
            return;
        };
        let Some(entry) = snapshot.entries.iter().find(|entry| entry.path == path) else {
            self.preview.select(None);
            ui.separator();
            ui.label("Resource details");
            ui.label(path);
            ui.label("Status: Missing from current index");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let (icon, color) = resource_icon(entry.kind, ui);
            ui.colored_label(color, icon);
            ui.label("Resource details");
            ui.label(format!("Path: {}", entry.path));
            ui.label(format!("Type: {}", kind_name(entry.kind)));
            ui.label(format!("Size: {} bytes", entry.size));
            ui.label(format!("Status: {}", validity_name(entry.validity)));
        });
        self.preview.select(Some((&entry.path, entry.kind)));
        egui::ScrollArea::vertical()
            .id_salt("project_resource_preview")
            .auto_shrink([false, false])
            .show(ui, |ui| self.preview.render(ui, entry.kind));
    }

    fn visit_directory(&mut self, path: &str) {
        if self.history.current() != Some(path) {
            self.history.visit(path);
        }
    }

    pub(super) fn current_directory(&self) -> &str {
        self.history.current().unwrap_or("")
    }

    fn operation_destination(&self) -> String {
        if self.current_directory().is_empty() {
            "assets".to_owned()
        } else {
            self.current_directory().to_owned()
        }
    }
}
