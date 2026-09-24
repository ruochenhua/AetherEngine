//! Particle emitter controls shown in the scene inspector.

use aether_engine::particles::ParticleEmitterConfig;

pub(super) fn render(ui: &mut egui::Ui, config: &mut ParticleEmitterConfig, restart: &mut bool) {
    ui.heading("Particle Emitter");
    ui.add(
        egui::DragValue::new(&mut config.emission_rate)
            .speed(0.5)
            .range(0.0..=10_000.0)
            .prefix("Rate/s: "),
    );
    ui.add(
        egui::DragValue::new(&mut config.burst)
            .speed(1.0)
            .range(0..=4096)
            .prefix("Burst: "),
    );
    ui.add(
        egui::DragValue::new(&mut config.seed)
            .speed(1.0)
            .prefix("Seed: "),
    );
    ui.checkbox(&mut config.paused, "Paused");
    if ui.button("Restart emitter").clicked() {
        *restart = true;
    }
    ui.label("Rate: particles per second; lifetime: seconds; speed: meters per second.");
}
