use super::{
    physics_system, Collider, ColliderList, ColliderShape, PhysicsDesc, PhysicsError,
    PhysicsRuntime, RigidBody, TransformAuthority,
};
use crate::ecs::{components::Transform, World};
use crate::time::{FrameTime, TimeSample};
use glam::Vec3;

#[path = "system_edge_tests.rs"]
mod edge;

fn sample(step_index: u64) -> TimeSample {
    let dt = 1.0 / 60.0;
    TimeSample {
        time: step_index as f32 * dt,
        dt,
        step_index,
    }
}

fn sphere() -> Collider {
    Collider {
        shape: ColliderShape::Sphere(0.5),
        ..Collider::default()
    }
}

#[test]
fn dynamic_sphere_follows_gravity_with_fixed_sample_steps() {
    let mut runtime = PhysicsRuntime::default();
    runtime
        .spawn_body(7, &Transform::default(), &RigidBody::default(), &sphere())
        .expect("valid sphere body should spawn");

    let samples = (1..=60).map(sample).collect::<Vec<_>>();
    let stats = runtime
        .step_frame(&FrameTime { samples, now: 1.0 })
        .expect("valid fixed samples should step");

    let (position, velocity) = runtime.body_state(7).expect("body should exist");
    assert_eq!(stats.substeps, 60);
    assert!((stats.simulated_time - 1.0).abs() <= 1.0e-4);
    // Frozen Rapier 0.22, f32, 60 × (1/60)s baseline. This intentionally
    // measures the engine integration, not an independently chosen Euler form.
    assert!((position.y - -4.9254365).abs() <= 1.0e-4);
    assert!((velocity.y - -9.81).abs() <= 1.0e-4);
}

#[test]
fn fixed_box_ignores_gravity_and_empty_frame_does_not_step() {
    let mut runtime = PhysicsRuntime::default();
    let fixed_body = RigidBody {
        is_static: true,
        ..RigidBody::default()
    };
    let box_collider = Collider {
        shape: ColliderShape::Box(Vec3::splat(0.5)),
        ..Collider::default()
    };
    let empty = runtime
        .step_frame(&FrameTime::default())
        .expect("empty samples are a no-op");
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), fixed_body, box_collider));
    let stepped = physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("one sample should run one step");

    let (position, velocity) = runtime
        .body_state(entity.to_bits().get())
        .expect("body should exist");
    assert_eq!(empty.substeps, 0);
    assert_eq!(stepped.substeps, 1);
    assert_eq!(position, Vec3::ZERO);
    assert_eq!(velocity, Vec3::ZERO);
}

#[test]
fn frame_samples_are_consumed_without_a_second_accumulator() {
    let mut runtime = PhysicsRuntime::default();
    runtime
        .spawn_body(11, &Transform::default(), &RigidBody::default(), &sphere())
        .expect("valid sphere body should spawn");

    let first = runtime
        .step_frame(&FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        })
        .expect("first sample should step");
    let second = runtime
        .step_frame(&FrameTime {
            samples: vec![sample(2), sample(3)],
            now: 3.0 / 60.0,
        })
        .expect("both samples should step");

    assert_eq!(first.substeps, 1);
    assert_eq!(second.substeps, 2);
    assert!((second.simulated_time - 2.0 / 60.0).abs() <= 1.0e-6);
}

#[test]
fn invalid_later_sample_rejects_the_whole_frame_without_partial_step() {
    let mut runtime = PhysicsRuntime::default();
    runtime
        .spawn_body(13, &Transform::default(), &RigidBody::default(), &sphere())
        .expect("valid sphere body should spawn");
    runtime
        .step_frame(&FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        })
        .expect("first sample should step");
    let before = runtime.body_state(13).expect("body should exist");

    let error = runtime
        .step_frame(&FrameTime {
            samples: vec![sample(2), sample(4)],
            now: 4.0 / 60.0,
        })
        .expect_err("discontinuous input must be rejected");

    assert!(matches!(
        error,
        PhysicsError::InvalidTimeSample {
            sample_index: 1,
            ..
        }
    ));
    assert_eq!(runtime.body_state(13), Some(before));
}

#[test]
fn dynamic_sphere_resolves_contact_with_fixed_box() {
    let mut runtime = PhysicsRuntime::default();
    let floor = Transform {
        translation: Vec3::new(0.0, -0.5, 0.0),
        ..Transform::default()
    };
    let floor_body = RigidBody {
        is_static: true,
        ..RigidBody::default()
    };
    let floor_shape = Collider {
        shape: ColliderShape::Box(Vec3::new(5.0, 0.5, 5.0)),
        friction: 0.8,
        ..Collider::default()
    };
    runtime
        .spawn_body(20, &floor, &floor_body, &floor_shape)
        .expect("fixed box floor should spawn");
    let falling = Transform {
        translation: Vec3::new(0.0, 2.0, 0.0),
        ..Transform::default()
    };
    runtime
        .spawn_body(21, &falling, &RigidBody::default(), &sphere())
        .expect("dynamic sphere should spawn");

    runtime
        .step_frame(&FrameTime {
            samples: (1..=180).map(sample).collect(),
            now: 3.0,
        })
        .expect("contact replay should complete");

    let (position, velocity) = runtime
        .body_state(21)
        .expect("sphere should remain present");
    assert!((position.y - 0.5).abs() <= 0.02);
    assert!(velocity.y.abs() <= 0.05);
}

#[test]
fn physics_system_writes_dynamic_pose_back_to_the_world() {
    let mut world = World::new();
    let entity = world.spawn((
        Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            ..Transform::default()
        },
        RigidBody::default(),
        sphere(),
    ));
    let mut runtime = PhysicsRuntime::default();

    physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("valid physics entity should sync and step");

    let transform = world.get::<&Transform>(entity).expect("entity remains");
    assert!(transform.translation.y < 2.0);
    drop(transform);
    assert_eq!(runtime.entity_count(), 1);

    runtime.set_paused(true);
    let paused = physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(2)],
            now: 2.0 / 60.0,
        },
    )
    .expect("pause should preserve registered state");
    assert_eq!(paused.substeps, 0);
    runtime.set_paused(false);
    let resumed = physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(2)],
            now: 2.0 / 60.0,
        },
    )
    .expect("resume should consume the next published sample");
    assert_eq!(resumed.substeps, 1);
}

#[test]
fn scene_authority_updates_body_pose_and_pause_does_not_step_or_sync_out() {
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        RigidBody {
            is_static: true,
            ..RigidBody::default()
        },
        sphere(),
    ));
    let mut runtime = PhysicsRuntime::default();
    physics_system(&mut runtime, &mut world, &FrameTime::default())
        .expect("first empty frame registers the scene body");
    world
        .get::<&mut Transform>(entity)
        .expect("entity remains")
        .translation = Vec3::new(0.0, 4.0, 0.0);
    runtime.set_paused(true);

    let stats = physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("paused sync should update scene-authoritative body only");

    assert_eq!(stats.substeps, 0);
    assert_eq!(runtime.body_state(entity.to_bits().get()).unwrap().0.y, 4.0);
    assert_eq!(world.get::<&Transform>(entity).unwrap().translation.y, 4.0);
}

#[test]
fn dynamic_entity_can_opt_into_scene_transform_authority() {
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        RigidBody::default(),
        sphere(),
        TransformAuthority::Scene,
    ));
    let mut runtime = PhysicsRuntime::default();
    runtime.set_paused(true);

    physics_system(&mut runtime, &mut world, &FrameTime::default())
        .expect("explicit authority should register");
    world.get::<&mut Transform>(entity).unwrap().translation.y = 3.0;
    physics_system(&mut runtime, &mut world, &FrameTime::default())
        .expect("paused scene-authoritative sync should update the body");

    assert_eq!(runtime.body_state(entity.to_bits().get()).unwrap().0.y, 3.0);
}

#[test]
fn multi_collider_remove_cascades_and_clears_reverse_mappings() {
    let mut runtime = PhysicsRuntime::default();
    let desc = PhysicsDesc {
        body: RigidBody::default(),
        colliders: vec![
            sphere(),
            sphere(),
            sphere(),
            Collider {
                shape: ColliderShape::Box(Vec3::splat(0.25)),
                ..Collider::default()
            },
        ],
        authority: TransformAuthority::Physics,
    };
    runtime
        .spawn_entity(71, &Transform::default(), &desc)
        .expect("two colliders should attach to one body");
    assert_eq!(runtime.entity_collider_count(71), Some(4));
    assert_eq!(runtime.collider_count(), 4);

    runtime
        .remove_entity(71)
        .expect("body removal should cascade");

    assert_eq!(runtime.entity_count(), 0);
    assert_eq!(runtime.collider_count(), 0);
    assert_eq!(runtime.body_count(), 0);
    assert!(matches!(
        runtime.remove_entity(71),
        Err(PhysicsError::StaleHandle { entity_bits: 71 })
    ));
}

#[test]
fn duplicate_or_invalid_multi_collider_spawn_leaves_previous_state_intact() {
    let mut runtime = PhysicsRuntime::default();
    let valid = PhysicsDesc {
        body: RigidBody::default(),
        colliders: vec![sphere()],
        authority: TransformAuthority::Physics,
    };
    runtime
        .spawn_entity(81, &Transform::default(), &valid)
        .expect("first spawn should succeed");
    let duplicate = runtime
        .spawn_entity(81, &Transform::default(), &valid)
        .expect_err("duplicate entity must fail");
    assert!(matches!(
        duplicate,
        PhysicsError::DuplicateEntity { entity_bits: 81 }
    ));

    let invalid = PhysicsDesc {
        colliders: vec![
            sphere(),
            Collider {
                shape: ColliderShape::Sphere(-1.0),
                ..Collider::default()
            },
        ],
        ..valid.clone()
    };
    assert!(matches!(
        runtime.spawn_entity(82, &Transform::default(), &invalid),
        Err(PhysicsError::InvalidShape {
            entity_bits: 82,
            ..
        })
    ));
    let too_many = PhysicsDesc {
        colliders: vec![sphere(); 5],
        ..valid
    };
    assert!(matches!(
        runtime.spawn_entity(83, &Transform::default(), &too_many),
        Err(PhysicsError::InvalidShape {
            entity_bits: 83,
            ..
        })
    ));
    assert_eq!(runtime.entity_count(), 1);
    assert_eq!(runtime.collider_count(), 1);
}

#[test]
fn sync_in_rebuilds_entity_when_rigid_body_configuration_changes() {
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), RigidBody::default(), sphere()));
    let mut runtime = PhysicsRuntime::default();
    physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("initial body should register");
    world.get::<&mut RigidBody>(entity).unwrap().velocity = Vec3::new(2.0, 0.0, 0.0);

    physics_system(&mut runtime, &mut world, &FrameTime::default())
        .expect("updated body description should reconcile");

    assert_eq!(
        runtime.body_state(entity.to_bits().get()).unwrap().1,
        Vec3::new(2.0, 0.0, 0.0)
    );
}

#[test]
fn world_despawn_and_reset_remove_bodies_and_allow_time_restart() {
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), RigidBody::default(), sphere()));
    let mut runtime = PhysicsRuntime::default();
    physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("first frame should create and step body");
    world.despawn(entity).expect("entity should despawn");
    physics_system(&mut runtime, &mut world, &FrameTime::default())
        .expect("next sync should remove stale body");
    assert_eq!(runtime.body_count(), 0);

    let replacement = world.spawn((Transform::default(), RigidBody::default(), sphere()));
    physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(2)],
            now: 2.0 / 60.0,
        },
    )
    .expect("step index remains monotonic before explicit reset");
    runtime
        .reset()
        .expect("reset should release Rapier handles");
    assert_eq!(runtime.entity_count(), 0);
    assert!(world.contains(replacement));
    physics_system(
        &mut runtime,
        &mut world,
        &FrameTime {
            samples: vec![sample(1)],
            now: 1.0 / 60.0,
        },
    )
    .expect("reset permits a new scene's step sequence");
    runtime.shutdown().expect("shutdown should be idempotent");
    assert_eq!(runtime.body_count(), 0);
}

#[test]
fn collider_list_component_registers_all_shapes_and_seek_replay_is_idempotent() {
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        RigidBody::default(),
        ColliderList(vec![
            sphere(),
            Collider {
                shape: ColliderShape::Box(Vec3::splat(0.25)),
                ..Collider::default()
            },
        ]),
    ));
    let frame = FrameTime {
        samples: (1..=10).map(sample).collect(),
        now: 10.0 / 60.0,
    };
    let mut runtime = PhysicsRuntime::default();

    let first = physics_system(&mut runtime, &mut world, &frame)
        .expect("multi-collider entity should sync and replay");
    let position = runtime.body_state(entity.to_bits().get()).unwrap().0;
    let repeated = physics_system(&mut runtime, &mut world, &frame)
        .expect("same deterministic seek frame should be reusable");

    assert_eq!(
        runtime.entity_collider_count(entity.to_bits().get()),
        Some(2)
    );
    assert_eq!(first.substeps, 10);
    assert_eq!(repeated.substeps, 0);
    assert_eq!(
        runtime.body_state(entity.to_bits().get()).unwrap().0,
        position
    );
}
