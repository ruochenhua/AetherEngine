//! Playback controls for the dedicated glTF animation acceptance actor.

use super::super::animation_fixture::PlaybackCommand;

#[derive(Clone, Debug)]
pub(super) struct Controls {
    pub(super) clip_name: String,
    pub(super) time: f32,
    pub(super) duration: f32,
    pub(super) playing: bool,
    pub(super) speed: f32,
    pub(super) looping: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Changes {
    pub(super) command: Option<PlaybackCommand>,
    pub(super) seek: Option<f32>,
}

pub(super) fn render(ui: &mut egui::Ui, controls: &mut Controls) -> Changes {
    let mut changes = Changes::default();
    egui::CollapsingHeader::new("Animation")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(format!("Clip: {}", controls.clip_name));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!controls.playing, egui::Button::new("▶ Play"))
                    .clicked()
                {
                    changes.command = Some(PlaybackCommand::Play);
                }
                if ui
                    .add_enabled(controls.playing, egui::Button::new("⏸ Pause"))
                    .clicked()
                {
                    changes.command = Some(PlaybackCommand::Pause);
                }
            });
            let duration = controls.duration.max(f32::EPSILON);
            if ui
                .add(egui::Slider::new(&mut controls.time, 0.0..=duration).text("Time (s)"))
                .changed()
            {
                changes.seek = Some(controls.time.min(controls.duration));
            }
            ui.add(egui::Slider::new(&mut controls.speed, -2.0..=2.0).text("Speed"));
            ui.checkbox(&mut controls.looping, "Loop");
        });
    changes
}

#[cfg(test)]
mod tests {
    use super::{render, Changes, Controls};

    #[test]
    fn animation_panel_renders_current_playback_without_mutating_state() {
        let context = egui::Context::default();
        let mut controls = Controls {
            clip_name: "RaiseArms".to_owned(),
            time: 0.5,
            duration: 0.75,
            playing: true,
            speed: 1.0,
            looping: true,
        };
        let mut changes = Changes::default();
        let _ = context.run_ui(Default::default(), |ui| {
            changes = render(ui, &mut controls);
        });

        assert_eq!(changes, Changes::default());
        assert_eq!(controls.time, 0.5);
        assert_eq!(controls.speed, 1.0);
        assert!(controls.playing);
        assert!(controls.looping);
    }
}
