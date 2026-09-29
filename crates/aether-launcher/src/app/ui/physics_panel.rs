//! Dedicated playback and collider-debug panel.

use super::super::particle_runtime::physics_runtime::{PlaybackAction, PlaybackState};

pub(super) fn render(
    ui: &mut egui::Ui,
    playback_state: PlaybackState,
    playback_action: &mut Option<PlaybackAction>,
    physics_debug_enabled: &mut bool,
    body_count: usize,
    collider_count: usize,
) -> bool {
    let response = egui::CollapsingHeader::new("Physics")
        .default_open(true)
        .show(ui, |ui| {
            let state_label = match playback_state {
                PlaybackState::Stopped => "Stopped",
                PlaybackState::Playing => "Playing",
                PlaybackState::Paused => "Paused",
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Simulation: {state_label}"));
                let play_label = if playback_state == PlaybackState::Paused {
                    "▶ Resume"
                } else {
                    "▶ Play"
                };
                let play_enabled = playback_state != PlaybackState::Playing;
                if ui
                    .add_enabled(play_enabled, egui::Button::new(play_label))
                    .clicked()
                {
                    *playback_action = Some(PlaybackAction::Play);
                }
                if ui
                    .add_enabled(
                        playback_state == PlaybackState::Playing,
                        egui::Button::new("⏸ Pause"),
                    )
                    .clicked()
                {
                    *playback_action = Some(PlaybackAction::Pause);
                }
                if ui
                    .add_enabled(
                        playback_state != PlaybackState::Stopped,
                        egui::Button::new("⏹ Stop"),
                    )
                    .clicked()
                {
                    *playback_action = Some(PlaybackAction::Stop);
                }
            });
            ui.label(format!(
                "Bodies: {body_count}  ·  Colliders: {collider_count}"
            ));
            egui::CollapsingHeader::new("Debug")
                .default_open(false)
                .show(ui, |ui| {
                    ui.checkbox(physics_debug_enabled, "Show collider wireframes");
                });
        });
    response.body_returned.is_some()
}

#[cfg(test)]
mod tests {
    use super::{render, PlaybackState};

    #[test]
    fn physics_controls_render_inline_in_the_expanded_section() {
        let context = egui::Context::default();
        let mut playback_action = None;
        let mut physics_debug_enabled = false;
        let mut section_expanded = false;

        let _ = context.run_ui(Default::default(), |ui| {
            section_expanded = render(
                ui,
                PlaybackState::Stopped,
                &mut playback_action,
                &mut physics_debug_enabled,
                1,
                1,
            );
        });

        assert!(
            section_expanded,
            "Physics controls should be visible inline by default"
        );
        assert_eq!(playback_action, None);
    }
}
