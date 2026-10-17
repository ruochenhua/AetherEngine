use super::{
    helpers::{kind_name, resource_icon, validity_name},
    ProjectPanel,
};
use crate::project_index::IndexSnapshot;

impl ProjectPanel {
    pub(super) fn render_details(&mut self, ui: &mut egui::Ui, snapshot: &IndexSnapshot) {
        let mut focus_preview = false;
        let entry = self
            .selected_resource
            .as_deref()
            .and_then(|path| snapshot.entries.iter().find(|entry| entry.path == path));
        if let Some(entry) = entry {
            ui.horizontal_wrapped(|ui| {
                let (icon, color) = resource_icon(entry.kind, ui);
                ui.colored_label(color, icon);
                ui.label("Resource details");
                ui.label(format!("Path: {}", entry.path));
                ui.label(format!("Type: {}", kind_name(entry.kind)));
                ui.label(format!("Size: {} bytes", entry.size));
                ui.label(format!("Status: {}", validity_name(entry.validity)));
            });
            if ui.button("Show References").clicked() {
                self.show_references();
                focus_preview = true;
            }
            self.preview.select(Some((&entry.path, entry.kind)));
        } else {
            self.preview.select(None);
            if let Some(path) = self.selected_resource.as_deref() {
                ui.label("Resource details");
                ui.label(path);
                ui.label("Status: Missing from current index");
            }
        }
        if self.references.target().is_some() {
            self.render_references(ui);
            if focus_preview {
                self.focus_references(ui);
            }
        } else if let Some(entry) = entry {
            egui::ScrollArea::vertical()
                .id_salt("project_resource_preview")
                .auto_shrink([false, false])
                .show(ui, |ui| self.preview.render(ui, entry.kind));
        }
    }
}
