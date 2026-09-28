//! Lazy launcher integration for the engine physics lifecycle.

use aether_engine::{
    ecs::{components::Transform, World},
    physics::{Collider, ColliderList, PhysicsRuntime, RigidBody},
    time::{TimeControl, TimeMode},
};
use tracing::error;

/// Advances physics before extraction, sharing a frame already published by
/// particles when both simulations are active.
pub(crate) fn advance(
    runtime: &mut Option<PhysicsRuntime>,
    world: &mut World,
    time: &mut TimeControl,
    wall_dt: f32,
    paused: bool,
    frame_already_published: bool,
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
pub(crate) fn scene_switched(runtime: &mut Option<PhysicsRuntime>) {
    if let Some(runtime) = runtime {
        if let Err(cause) = runtime.reset() {
            error!("Physics runtime scene reset failed: {cause}");
        }
    }
    *runtime = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{ecs::components::Transform, physics::ColliderShape};
    use glam::Vec3;

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

        advance(&mut runtime, &mut world, &mut time, 0.25, false, true)
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

        advance(&mut runtime, &mut world, &mut time, 0.0, false, false)
            .expect("initial seek should simulate");
        let after_first_target = world
            .query::<&Transform>()
            .iter()
            .next()
            .unwrap()
            .translation;
        time.seek_frame(0.2).expect("publish a new target");
        advance(&mut runtime, &mut world, &mut time, 0.0, false, false)
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
        )
        .expect("physics runtime should be created");

        scene_switched(&mut runtime);

        assert!(runtime.is_none());
    }
}
