use super::super::animation::AnimatorControls;

pub(super) fn render(ui: &mut egui::Ui, controls: &mut AnimatorControls) {
    ui.heading("Animation");
    ui.horizontal(|ui| {
        if ui
            .button(if controls.playing { "Pause" } else { "Play" })
            .clicked()
        {
            controls.playing = !controls.playing;
        }
        ui.checkbox(&mut controls.looping, "Loop");
    });
    ui.add(
        egui::Slider::new(&mut controls.time, 0.0..=controls.duration.max(0.0)).text("Time (s)"),
    );
    ui.add(egui::Slider::new(&mut controls.speed, 0.0..=4.0).text("Speed"));
    ui.separator();
}
