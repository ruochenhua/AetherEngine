//! Session state and shared bottom panels for the editor workspace.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkspaceTab {
    #[default]
    Project,
    Log,
}

pub(crate) struct Workspace {
    pub(crate) tab: WorkspaceTab,
    pub(crate) visible: bool,
    pub(crate) height: f32,
    /// Asset details never replace the scene entity's Selected component.
    pub(crate) selected_resource: Option<PathBuf>,
    pub(crate) search: String,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            tab: WorkspaceTab::Project,
            visible: true,
            height: 180.0,
            selected_resource: None,
            search: String::new(),
        }
    }
}

impl Workspace {
    pub(crate) fn select_tab(&mut self, tab: WorkspaceTab) {
        self.tab = tab;
        self.visible = true;
    }

    pub(crate) fn select_resource(&mut self, path: Option<PathBuf>) {
        self.selected_resource = path;
    }

    pub(crate) fn close_resource(&mut self) {
        self.select_resource(None);
    }

    pub(crate) fn render(&mut self, ui: &mut egui::Ui, message: Option<&str>) {
        egui::Panel::top("workspace_controls").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.visible, "Workspace");
                if ui.button("Project").clicked() {
                    self.select_tab(WorkspaceTab::Project);
                }
                if ui.button("Log").clicked() {
                    self.select_tab(WorkspaceTab::Log);
                }
            });
        });
        if !self.visible {
            return;
        }
        let maximum = (ui.available_height() * 0.6).max(100.0);
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
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    WorkspaceTab::Project => {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search resources"),
                        );
                        ui.label("Project resources will appear here.");
                        if let Some(path) = self.selected_resource.as_ref() {
                            ui.separator();
                            ui.label("Resource details");
                            ui.label(path.display().to_string());
                            if ui.button("Close resource details").clicked() {
                                self.close_resource();
                            }
                        }
                    }
                    WorkspaceTab::Log => {
                        if let Some(message) = message {
                            ui.label(message);
                        } else {
                            ui.label("No log messages yet.");
                        }
                    }
                });
            });
        self.height = panel.response.rect.height();
    }
}
