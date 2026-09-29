//! Lazy launcher integration for the engine physics lifecycle.

use aether_engine::{
    ecs::{components::Transform, Entity, World},
    physics::{Collider, ColliderList, PhysicsRuntime, RigidBody},
    time::{TimeControl, TimeMode},
};
use std::collections::BTreeMap;
use tracing::error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybackAction {
    Play,
    Pause,
    Stop,
}

impl PlaybackState {
    /// Applies a transport action and returns whether it starts a fresh run.
    pub(crate) fn apply(&mut self, action: PlaybackAction) -> bool {
        match action {
            PlaybackAction::Play => {
                let restart = *self == Self::Stopped;
                *self = Self::Playing;
                restart
            }
            PlaybackAction::Pause if *self == Self::Playing => {
                *self = Self::Paused;
                false
            }
            PlaybackAction::Stop if *self != Self::Stopped => {
                *self = Self::Stopped;
                true
            }
            PlaybackAction::Pause | PlaybackAction::Stop => false,
        }
    }
}

/// Scene-authored poses captured immediately before simulation starts.
#[derive(Clone, Debug, Default)]
pub(crate) struct SceneSimulationSnapshot {
    physics_transforms: BTreeMap<u64, (Entity, Transform)>,
}

impl SceneSimulationSnapshot {
    pub(crate) fn capture(world: &World) -> Self {
        let mut physics_transforms = BTreeMap::new();
        for (entity, transform, _, _) in world
            .query::<(Entity, &Transform, &RigidBody, &Collider)>()
            .iter()
        {
            physics_transforms.insert(entity.to_bits().get(), (entity, transform.clone()));
        }
        for (entity, transform, _, _) in world
            .query::<(Entity, &Transform, &RigidBody, &ColliderList)>()
            .iter()
        {
            physics_transforms.insert(entity.to_bits().get(), (entity, transform.clone()));
        }
        Self { physics_transforms }
    }

    pub(crate) fn restore(&self, world: &mut World) {
        for (entity, transform) in world.query_mut::<(Entity, &mut Transform)>().into_iter() {
            if let Some((_, initial)) = self.physics_transforms.get(&entity.to_bits().get()) {
                *transform = initial.clone();
            }
        }
    }
}

/// Advances physics before extraction, sharing a frame already published by
/// particles when both simulations are active.
pub(crate) fn advance(
    runtime: &mut Option<PhysicsRuntime>,
    world: &mut World,
    time: &mut TimeControl,
    wall_dt: f32,
    paused: bool,
    frame_already_published: bool,
    debug_enabled: bool,
) -> Result<(), String> {
    let has_physics_entities = world
        .query::<(&Transform, &RigidBody, &Collider)>()
        .iter()
        .next()
        .is_some()
        || world
            .query::<(&Transform, &RigidBody, &ColliderList)>()
            .iter()
            .next()
            .is_some();
    if !has_physics_entities {
        if let Some(runtime) = runtime {
            runtime.shutdown().map_err(|error| error.to_string())?;
        }
        *runtime = None;
        return Ok(());
    }

    let runtime = runtime.get_or_insert_with(PhysicsRuntime::default);
    runtime.set_paused(paused);
    runtime.set_debug_enabled(debug_enabled);
    let frame = if time.mode == TimeMode::Seek {
        if time.published_frame().samples.is_empty() && time.simulation_time > 0.0 {
            time.seek_frame(time.simulation_time)
                .map_err(|error| error.to_string())?;
        }
        time.published_frame().clone()
    } else if frame_already_published {
        time.published_frame().clone()
    } else {
        time.advance_frame(if paused { 0.0 } else { wall_dt })
            .map_err(|error| error.to_string())?
    };

    if time.mode == TimeMode::Seek {
        runtime
            .prepare_seek_frame(world, &frame)
            .map_err(|error| error.to_string())?;
    }

    aether_engine::physics::physics_system(runtime, world, &frame)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Drops old Rapier handles after a successful scene replacement.
pub(crate) fn scene_switched(simulation: &mut super::SimulationRuntime) {
    if let Some(runtime) = simulation.physics.as_mut() {
        if let Err(cause) = runtime.reset() {
            error!("Physics runtime scene reset failed: {cause}");
        }
    }
    simulation.physics = None;
    simulation.playback = PlaybackState::Stopped;
    simulation.scene_snapshot = None;
    simulation.pending_action = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{ecs::components::Transform, physics::ColliderShape};
    use glam::Vec3;

    #[test]
    fn physics_playback_resumes_when_paused_and_restarts_after_stop() {
        let mut playback = PlaybackState::Stopped;

        assert!(playback.apply(PlaybackAction::Play));
        assert_eq!(playback, PlaybackState::Playing);
        assert!(!playback.apply(PlaybackAction::Pause));
        assert_eq!(playback, PlaybackState::Paused);
        assert!(!playback.apply(PlaybackAction::Play));
        assert_eq!(playback, PlaybackState::Playing);
        assert!(playback.apply(PlaybackAction::Stop));
        assert_eq!(playback, PlaybackState::Stopped);
        assert!(playback.apply(PlaybackAction::Play));
    }

    #[test]
    fn stopping_physics_restores_the_pose_captured_before_play() {
        let mut world = physics_world();
        let entity = world.query::<Entity>().iter().next().unwrap();
        let snapshot = SceneSimulationSnapshot::capture(&world);
        world.get::<&mut Transform>(entity).unwrap().translation.y = -12.0;

        snapshot.restore(&mut world);

        assert_eq!(
            world.get::<&Transform>(entity).unwrap().translation,
            Vec3::Y
        );
    }

    fn physics_world() -> World {
        let mut world = World::new();
        world.spawn((
            Transform {
                translation: Vec3::Y,
                ..Transform::default()
            },
            RigidBody::default(),
            Collider {
                shape: ColliderShape::Sphere(0.5),
                ..Collider::default()
            },
        ));
        world
    }

    #[test]
    fn launcher_lazily_creates_physics_and_advances_shared_clock_once() {
        let mut world = physics_world();
        let mut runtime = None;
        let mut time = TimeControl::default();

        advance(
            &mut runtime,
            &mut world,
            &mut time,
            1.0 / 60.0,
            false,
            false,
            false,
        )
        .expect("physics should initialize and step");

        assert_eq!(runtime.as_ref().unwrap().entity_count(), 1);
        assert_eq!(time.frame_index, 1);
    }

    #[test]
    fn physics_reuses_particle_published_frame_without_advancing_time_twice() {
        let mut world = physics_world();
        let mut runtime = None;
        let mut time = TimeControl::default();
        time.advance_frame(0.25).expect("publish a shared frame");
        let published_index = time.frame_index;

        advance(
            &mut runtime,
            &mut world,
            &mut time,
            0.25,
            false,
            true,
            false,
        )
        .expect("physics should consume the published particle frame");

        assert_eq!(published_index, 4);
        assert_eq!(time.frame_index, published_index);
        assert_eq!(runtime.as_ref().unwrap().entity_count(), 1);
    }

    #[test]
    fn scenes_without_physics_do_not_allocate_a_runtime() {
        let mut runtime = None;
        let mut time = TimeControl::default();

        advance(
            &mut runtime,
            &mut World::new(),
            &mut time,
            1.0 / 60.0,
            false,
            false,
            false,
        )
        .expect("empty scenes should be a no-op");

        assert!(runtime.is_none());
        assert_eq!(time.frame_index, 0);
    }

    #[test]
    fn changing_seek_target_replays_from_initial_scene_pose() {
        let mut world = physics_world();
        let mut runtime = None;
        let mut time = TimeControl::new(TimeMode::Seek, 0.1, 1.0 / 60.0, 4, 4096)
            .expect("valid seek configuration");

        advance(
            &mut runtime,
            &mut world,
            &mut time,
            0.0,
            false,
            false,
            false,
        )
        .expect("initial seek should simulate");
        let after_first_target = world
            .query::<&Transform>()
            .iter()
            .next()
            .unwrap()
            .translation;
        time.seek_frame(0.2).expect("publish a new target");
        advance(
            &mut runtime,
            &mut world,
            &mut time,
            0.0,
            false,
            false,
            false,
        )
        .expect("new seek should restart physics from the scene pose");
        let changed_runtime_pose = world
            .query::<&Transform>()
            .iter()
            .next()
            .unwrap()
            .translation;

        let mut fresh_world = physics_world();
        let mut fresh_runtime = None;
        let mut fresh_time = TimeControl::new(TimeMode::Seek, 0.2, 1.0 / 60.0, 4, 4096)
            .expect("valid seek configuration");
        advance(
            &mut fresh_runtime,
            &mut fresh_world,
            &mut fresh_time,
            0.0,
            false,
            false,
            false,
        )
        .expect("fresh runtime should produce the comparison pose");
        let fresh_pose = fresh_world
            .query::<&Transform>()
            .iter()
            .next()
            .unwrap()
            .translation;

        assert!(after_first_target.y < 1.0);
        assert_eq!(changed_runtime_pose, fresh_pose);
    }

    #[test]
    fn scene_switch_releases_the_previous_physics_runtime() {
        let mut world = physics_world();
        let mut runtime = None;
        let mut time = TimeControl::default();
        advance(
            &mut runtime,
            &mut world,
            &mut time,
            1.0 / 60.0,
            false,
            false,
            false,
        )
        .expect("physics runtime should be created");

        let mut simulation = crate::app::particle_runtime::SimulationRuntime {
            physics: runtime,
            ..Default::default()
        };
        scene_switched(&mut simulation);
        assert!(simulation.physics.is_none());
        assert_eq!(simulation.playback, PlaybackState::Stopped);
    }
}
