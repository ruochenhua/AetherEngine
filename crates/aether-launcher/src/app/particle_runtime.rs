//! Scene and time-control integration for the deterministic particle runtime.

use aether_engine::{
    ecs::{Entity, World},
    particles::{ParticleEmitterConfig, ParticleFrame, ParticleRuntime},
    time::{TimeControl, TimeMode},
};
use std::sync::Arc;

#[path = "physics_runtime.rs"]
pub(crate) mod physics_runtime;

#[derive(Default)]
pub(crate) struct SimulationRuntime {
    pub(crate) particles: ParticleRuntime,
    pub(crate) physics: Option<aether_engine::physics::PhysicsRuntime>,
    pub(crate) playback: physics_runtime::PlaybackState,
    pub(crate) scene_snapshot: Option<physics_runtime::SceneSimulationSnapshot>,
    pub(crate) pending_action: Option<physics_runtime::PlaybackAction>,
}

impl SimulationRuntime {
    pub(crate) fn new(auto_play: bool) -> Self {
        Self {
            playback: if auto_play {
                physics_runtime::PlaybackState::Playing
            } else {
                physics_runtime::PlaybackState::Stopped
            },
            ..Self::default()
        }
    }

    pub(crate) fn configure(&mut self, world: &World) -> Result<(), String> {
        configure_from_world(&mut self.particles, world)
    }
}

pub(super) fn configure_from_world(
    runtime: &mut ParticleRuntime,
    world: &World,
) -> Result<(), String> {
    let mut emitters: Vec<_> = world
        .query::<(Entity, &ParticleEmitterConfig)>()
        .iter()
        .map(|(entity, config)| {
            let mut config = config.clone();
            config.entity_bits = entity.to_bits().get();
            config
        })
        .collect();
    emitters.sort_by_key(|config| config.entity_bits);
    runtime
        .configure_emitters(emitters)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(super) fn load_scene(
    runtime: &mut ParticleRuntime,
    world: &World,
    time: &mut TimeControl,
) -> Result<Arc<ParticleFrame>, String> {
    *runtime = ParticleRuntime::default();
    configure_from_world(runtime, world)?;
    reset_time(time)?;
    initialize_time(runtime, time)
}

pub(super) fn restart_emitter(
    runtime: &mut ParticleRuntime,
    time: &mut TimeControl,
    entity_bits: u64,
) -> Result<Arc<ParticleFrame>, String> {
    let mode = time.mode;
    let target = if mode == TimeMode::Seek {
        time.simulation_time
    } else {
        0.0
    };
    runtime
        .restart_emitter(entity_bits)
        .map_err(|error| error.to_string())?;
    reset_time_to(time, target)?;
    initialize_time(runtime, time)
}

fn reset_time(time: &mut TimeControl) -> Result<(), String> {
    let target = if time.mode == TimeMode::Seek {
        time.simulation_time
    } else {
        0.0
    };
    reset_time_to(time, target)
}

fn reset_time_to(time: &mut TimeControl, target: f32) -> Result<(), String> {
    *time = TimeControl::new(
        time.mode,
        target,
        time.fixed_dt,
        time.max_substeps,
        time.max_seek_steps,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn initialize_time(
    runtime: &mut ParticleRuntime,
    time: &mut TimeControl,
) -> Result<Arc<ParticleFrame>, String> {
    if time.mode == TimeMode::Seek {
        time.seek_and_step(time.simulation_time, runtime)
            .map_err(|error| error.to_string())?;
    }
    Ok(runtime.current_frame().clone())
}

pub(super) fn advance(
    runtime: &mut ParticleRuntime,
    time: &mut TimeControl,
    wall_dt: f32,
) -> Result<Arc<ParticleFrame>, String> {
    if time.mode != TimeMode::Seek {
        time.advance_and_step(wall_dt, runtime)
            .map_err(|error| error.to_string())?;
    }
    Ok(runtime.current_frame().clone())
}

pub(super) fn is_static_billboard_fixture(scene_path: Option<&str>) -> bool {
    scene_path.is_some_and(|path| path.ends_with("t5_particle_billboard.ron"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{particles::FrameStatus, time::TimeMode};

    #[test]
    fn editor_transport_starts_stopped_while_automated_runs_autoplay() {
        assert_eq!(
            SimulationRuntime::new(false).playback,
            physics_runtime::PlaybackState::Stopped
        );
        assert_eq!(
            SimulationRuntime::new(true).playback,
            physics_runtime::PlaybackState::Playing
        );
    }

    fn configured_world() -> World {
        let mut world = World::new();
        world.spawn((ParticleEmitterConfig {
            emission_rate: 30.0,
            burst: 2,
            lifetime: [2.0, 2.0],
            seed: 91,
            ..Default::default()
        },));
        world
    }

    #[test]
    fn seek_time_publishes_repeatable_particle_frames() {
        let world = configured_world();
        let mut first = ParticleRuntime::default();
        let mut second = ParticleRuntime::default();
        configure_from_world(&mut first, &world).unwrap();
        configure_from_world(&mut second, &world).unwrap();
        let mut time_a = TimeControl::new(TimeMode::Seek, 0.5, 1.0 / 60.0, 4, 4096).unwrap();
        let mut time_b = time_a.clone();

        initialize_time(&mut first, &mut time_a).unwrap();
        initialize_time(&mut second, &mut time_b).unwrap();

        assert_eq!(
            first.current_frame().state_hash,
            second.current_frame().state_hash
        );
        assert_eq!(first.current_frame().items, second.current_frame().items);
        assert!(!first.current_frame().items.is_empty());
    }

    #[test]
    fn scene_without_emitters_publishes_an_empty_frame() {
        let world = World::new();
        let mut runtime = ParticleRuntime::default();
        configure_from_world(&mut runtime, &world).unwrap();
        assert_eq!(runtime.current_frame().status, FrameStatus::Empty);
        assert!(runtime.current_frame().items.is_empty());
    }

    #[test]
    fn effect_acceptance_scenes_have_valid_emitter_configurations() {
        let scenes = [
            include_str!("../../../../scenes/t5_particle_fire.ron"),
            include_str!("../../../../scenes/t5_particle_smoke.ron"),
            include_str!("../../../../scenes/t5_particle_dust.ron"),
        ];
        for contents in scenes {
            let scene = aether_engine::scene::SceneDescription::from_ron(contents).unwrap();
            assert_eq!(scene.particle_emitters.len(), 1);
            assert_ne!(scene.particle_emitters[0].seed, 0);
            assert!(scene.particle_emitters[0].burst > 0);
        }
    }

    #[test]
    fn static_billboard_fixture_is_limited_to_its_acceptance_scene() {
        assert!(is_static_billboard_fixture(Some(
            "scenes/t5_particle_billboard.ron"
        )));
        assert!(!is_static_billboard_fixture(Some(
            "scenes/t5_particle_fire.ron"
        )));
        assert!(!is_static_billboard_fixture(None));
    }

    #[test]
    fn restarting_emitter_resets_time_indices_before_the_next_live_step() {
        let world = configured_world();
        let mut runtime = ParticleRuntime::default();
        configure_from_world(&mut runtime, &world).unwrap();
        let mut time = TimeControl::default();
        advance(&mut runtime, &mut time, 1.0 / 60.0).unwrap();
        let emitter_id = world
            .query::<Entity>()
            .iter()
            .next()
            .unwrap()
            .to_bits()
            .get();

        restart_emitter(&mut runtime, &mut time, emitter_id).unwrap();
        let next = advance(&mut runtime, &mut time, 1.0 / 60.0).unwrap();
        assert_eq!(time.frame_index, 1);
        assert!(!next.items.is_empty());
    }
}
