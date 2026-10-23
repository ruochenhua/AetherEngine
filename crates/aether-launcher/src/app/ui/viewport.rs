use super::{asset_drop, viewport_tips};

pub(super) fn render(
    ui: &mut egui::Ui,
    scroll_input: &mut f32,
    drop_request: &mut Option<super::super::workspace::ViewportDropRequest>,
    tips: &mut viewport_tips::ViewportTipsState,
) -> bool {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| {
            let hovered = asset_drop::capture_viewport_input(ui, scroll_input, drop_request);
            viewport_tips::render(ui, tips);
            hovered
        })
        .inner
}
