//! Compact, mode-aware controls help anchored to the scene viewport.

#[derive(Debug)]
pub(crate) struct ViewportTipsState {
    pub(crate) expanded: bool,
}

impl Default for ViewportTipsState {
    fn default() -> Self {
        Self { expanded: true }
    }
}

pub(crate) fn render(ui: &mut egui::Ui, state: &mut ViewportTipsState) {
    let rect = egui::Rect::from_min_size(
        ui.max_rect().min + egui::vec2(14.0, 14.0),
        egui::vec2(320.0, 270.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui::Frame::popup(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_min_width(270.0);
                if !state.expanded {
                    if ui.small_button("? Viewport").clicked() {
                        state.expanded = true;
                    }
                    return;
                }

                ui.horizontal(|ui| {
                    ui.heading("Viewport Controls");
                    ui.label(egui::RichText::new("Edit").weak().small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            state.expanded = false;
                        }
                    });
                });
                ui.separator();
                control(ui, "LMB", "Select / manipulate");
                control(ui, "LMB drag", "Look around");
                control(ui, "LMB + WASD/QE", "Fly camera");
                control(ui, "MMB drag", "Pan");
                control(ui, "Wheel / scroll", "Dolly");
                control(ui, "Shift + scroll", "Pan vertically");
                control(ui, "F", "Frame selection");
                control(ui, "Option + LMB drag", "Orbit selected");
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new("× Hide; click ? to show again.")
                        .weak()
                        .small(),
                );
            });
    });
}

fn control(ui: &mut egui::Ui, gesture: &str, action: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [112.0, 18.0],
            egui::Label::new(egui::RichText::new(gesture).monospace().small()),
        );
        ui.label(egui::RichText::new(action).small());
    });
}
