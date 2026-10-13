//! Session state and shared bottom panels for the editor workspace.

use std::path::PathBuf;

mod project_panel;

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

    pub(crate) fn render(
        &mut self,
        ui: &mut egui::Ui,
        message: Option<&str>,
        logs: &mut crate::logging::LogPanel,
    ) -> Option<PathBuf> {
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
            return None;
        }
        let maximum = (ui.available_height() * 0.6).max(100.0);
        let mut pending_open = None;
        let panel = egui::Panel::bottom("editor_workspace")
            .default_size(self.height)
            .size_range(100.0..=maximum)
            .resizable(true)
            .show_inside(ui, |ui| {
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
                    pending_open = self.project.render(ui);
                    self.selected_resource = self.project.selected_resource().map(PathBuf::from);
                }
            });
        self.height = panel.response.rect.height();
        pending_open
    }
}
