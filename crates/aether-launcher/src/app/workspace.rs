//! Session state and shared bottom panels for the editor workspace.

use std::path::PathBuf;

mod project_panel;
pub(crate) use project_panel::AsyncWorkSnapshot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkspaceTab {
    #[default]
    Project,
    Log,
}

/// egui-native Project asset drag payload.
#[derive(Clone, Debug)]
pub(crate) struct AssetDragPayload {
    pub(crate) relative_path: String,
    pub(crate) kind: crate::project_index::ResourceKind,
}

/// Physical pointer position plus the central viewport's logical hit rectangle.
#[derive(Clone, Debug)]
pub(crate) struct ViewportDropRequest {
    pub(crate) relative_path: String,
    pub(crate) pointer_physical: [f32; 2],
    pub(crate) viewport_points_min: [f32; 2],
    pub(crate) viewport_points_max: [f32; 2],
    pub(crate) pixels_per_point: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialDropTarget {
    Asset,
    Texture(aether_engine::scene::material::TextureUsage),
}

#[derive(Clone, Debug)]
pub(crate) struct MaterialDropRequest {
    pub(crate) entity: aether_engine::ecs::Entity,
    pub(crate) target: MaterialDropTarget,
    pub(crate) resource: AssetDragPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProjectFileAction {
    CreateFolder { destination: String, name: String },
    ImportAsset { destination: String },
}

#[derive(Default)]
pub(crate) struct WorkspaceRequests {
    pub(crate) open_scene: Option<PathBuf>,
    pub(crate) project_file: Option<ProjectFileAction>,
    pub(crate) confirm_import: bool,
    pub(crate) cancel_import: bool,
}

pub(crate) struct Workspace {
    pub(crate) tab: WorkspaceTab,
    pub(crate) visible: bool,
    pub(crate) height: f32,
    /// Asset details never replace the scene entity's Selected component.
    pub(crate) selected_resource: Option<PathBuf>,
    project: project_panel::ProjectPanel,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            tab: WorkspaceTab::Project,
            visible: true,
            height: 260.0,
            selected_resource: None,
            project: project_panel::ProjectPanel::new(
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ),
        }
    }
}

impl Workspace {
    pub(crate) fn project_root(&self) -> &std::path::Path {
        self.project.root()
    }
    pub(crate) fn project_async_work(&self) -> project_panel::AsyncWorkSnapshot {
        self.project.async_work_snapshot()
    }
    #[cfg(test)]
    pub(crate) fn set_project_root(&mut self, root: PathBuf) {
        self.project = project_panel::ProjectPanel::new(root);
        self.project.wait_for_refresh();
    }

    #[cfg(test)]
    pub(crate) fn project_directory(&self) -> &str {
        self.project.current_directory()
    }

    pub(crate) fn select_tab(&mut self, tab: WorkspaceTab) {
        self.tab = tab;
        self.visible = true;
    }

    pub(crate) fn select_resource(&mut self, path: Option<PathBuf>) {
        self.selected_resource = path.clone();
        self.project.select_resource(path);
    }

    pub(crate) fn refresh_project(&mut self) {
        self.project.refresh();
    }

    pub(crate) fn import_destination(&self) -> Option<String> {
        if self.tab != WorkspaceTab::Project || !self.visible {
            return None;
        }
        let current = self.project.current_directory();
        (current == "assets" || current.starts_with("assets/")).then(|| current.to_owned())
    }

    pub(crate) fn asset_drop_target_rect(&self) -> Option<egui::Rect> {
        self.import_destination()?;
        self.project.asset_drop_target_rect()
    }

    pub(crate) fn accepts_asset_drop_at(&self, position: egui::Pos2) -> bool {
        self.asset_drop_target_rect()
            .is_some_and(|rect| rect.contains(position))
    }

    pub(crate) fn render(
        &mut self,
        ui: &mut egui::Ui,
        message: Option<&str>,
        logs: &mut crate::logging::LogPanel,
        pending_import: Option<(&str, &[PathBuf])>,
    ) -> WorkspaceRequests {
        let mut requests = WorkspaceRequests::default();
        egui::Panel::top("workspace_controls").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if self.visible {
                    ui.toggle_value(&mut self.visible, "Workspace");
                } else {
                    if ui.button("Project").clicked() {
                        self.select_tab(WorkspaceTab::Project);
                    }
                    if ui.button("Log").clicked() {
                        self.select_tab(WorkspaceTab::Log);
                    }
                }
            });
        });
        if !self.visible {
            return requests;
        }
        let maximum = (ui.available_height() * 0.6).max(100.0);
        let panel = egui::Panel::bottom("editor_workspace")
            .default_size(self.height)
            .size_range(100.0..=maximum)
            .resizable(true)
            .show_inside(ui, |ui| {
                if let Some((destination, sources)) = pending_import {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.heading("Confirm Asset Import");
                        if sources.len() == 1 {
                            ui.label(format!("File: {}", sources[0].display()));
                        } else {
                            ui.label(format!("Files: {} selected", sources.len()));
                            for source in sources.iter().take(5) {
                                ui.small(source.display().to_string());
                            }
                            if sources.len() > 5 {
                                ui.small(format!("…and {} more", sources.len() - 5));
                            }
                        }
                        ui.label(format!(
                            "Target: {}",
                            if destination.is_empty() {
                                "."
                            } else {
                                destination
                            }
                        ));
                        ui.label("Supported formats: PNG, JPEG, self-contained GLB");
                        ui.horizontal(|ui| {
                            if ui.button("Import").clicked() {
                                requests.confirm_import = true;
                            }
                            if ui.button("Cancel").clicked() {
                                requests.cancel_import = true;
                            }
                        });
                    });
                }
                ui.take_available_space();
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.tab == WorkspaceTab::Project, "Project")
                        .clicked()
                    {
                        self.select_tab(WorkspaceTab::Project);
                    }
                    if ui
                        .selectable_label(self.tab == WorkspaceTab::Log, "Log")
                        .clicked()
                    {
                        self.select_tab(WorkspaceTab::Log);
                    }
                    if ui.button("Hide").clicked() {
                        self.visible = false;
                    }
                });
                ui.separator();
                if self.tab == WorkspaceTab::Log {
                    if let Some(message) = message {
                        ui.label(message);
                    }
                    if let Some(path) = logs.render(ui) {
                        self.select_resource(Some(path));
                        self.select_tab(WorkspaceTab::Project);
                    }
                } else {
                    if let Some(message) = message {
                        ui.label(message);
                    }
                    let (pending_open, project_file) = self.project.render(ui);
                    requests.open_scene = pending_open;
                    requests.project_file = project_file;
                    self.selected_resource = self.project.selected_resource().map(PathBuf::from);
                }
            });
        self.height = panel.response.rect.height();
        requests
    }
}
