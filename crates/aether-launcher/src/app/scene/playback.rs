//! Scene simulation transport lifecycle.

use crate::app::{particle_runtime::physics_runtime::*, App, LauncherState};

pub(super) fn process_pending(app: &mut App) {
    let Some(action) = app.particle_runtime.pending_action.take() else {
        return;
    };
    let restart = app.particle_runtime.playback.apply(action);
    if restart && action == PlaybackAction::Play {
        if let LauncherState::Running { ref world, .. } = app.state {
            app.particle_runtime.scene_snapshot = Some(SceneSimulationSnapshot::capture(world));
        }
        return;
    }
    if !restart || action != PlaybackAction::Stop {
        return;
    }

    if let LauncherState::Running { ref mut world, .. } = app.state {
        if let Some(snapshot) = app.particle_runtime.scene_snapshot.take() {
            snapshot.restore(world);
        }
        crate::app::particle_runtime::physics_runtime::scene_switched(&mut app.particle_runtime);
        if let Err(error) = crate::app::particle_runtime::load_scene(
            &mut app.particle_runtime.particles,
            world,
            &mut app.cli.time,
        ) {
            tracing::error!("Simulation reset after Stop failed: {error}");
        }
    }
}
