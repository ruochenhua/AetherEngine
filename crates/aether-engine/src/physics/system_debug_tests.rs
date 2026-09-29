use super::*;
use crate::renderer::picking::Ray;
use rapier3d::prelude::QueryFilter;
use sha2::{Digest, Sha256};

#[test]
fn ray_cast_returns_closest_entity_and_surface_normal() {
    let mut runtime = PhysicsRuntime::default();
    let near = Transform {
        translation: Vec3::new(0.0, 0.0, 0.0),
        ..Transform::default()
    };
    let far = Transform {
        translation: Vec3::new(0.0, 0.0, -2.0),
        ..Transform::default()
    };
    runtime
        .spawn_body(10, &near, &RigidBody::default(), &sphere())
        .expect("near collider should spawn");
    runtime
        .spawn_body(20, &far, &RigidBody::default(), &sphere())
        .expect("far collider should spawn");

    let hit = runtime
        .cast_ray(
            &Ray {
                origin: Vec3::new(0.0, 0.0, 3.0),
                dir: -Vec3::Z,
            },
            10.0,
            QueryFilter::default(),
        )
        .expect("valid ray should query")
        .expect("ray should hit a sphere");
    assert_eq!(hit.entity_bits, 10);
    assert!((hit.toi - 2.5).abs() < 1.0e-4);
    assert!(hit.normal.abs_diff_eq(Vec3::Z, 1.0e-4));
}

#[test]
fn ray_cast_can_skip_trigger_colliders_for_editor_picking() {
    let mut runtime = PhysicsRuntime::default();
    let mut trigger = sphere();
    trigger.is_trigger = true;
    runtime
        .spawn_body(
            11,
            &Transform {
                translation: Vec3::new(0.0, 0.0, 1.0),
                ..Transform::default()
            },
            &RigidBody::default(),
            &trigger,
        )
        .expect("trigger should spawn");
    runtime
        .spawn_body(12, &Transform::default(), &RigidBody::default(), &sphere())
        .expect("solid collider should spawn");

    let hit = runtime
        .cast_ray(
            &Ray {
                origin: Vec3::new(0.0, 0.0, 3.0),
                dir: -Vec3::Z,
            },
            10.0,
            QueryFilter::default().exclude_sensors(),
        )
        .expect("valid ray should query")
        .expect("ray should hit the solid behind the trigger");
    assert_eq!(hit.entity_bits, 12);
}

#[test]
fn ray_cast_rejects_invalid_ray_values() {
    let runtime = PhysicsRuntime::default();
    for ray in [
        Ray {
            origin: Vec3::splat(f32::NAN),
            dir: Vec3::Z,
        },
        Ray {
            origin: Vec3::ZERO,
            dir: Vec3::ZERO,
        },
    ] {
        assert_eq!(
            runtime.cast_ray(&ray, 10.0, QueryFilter::default()),
            Err(PhysicsError::InvalidRay)
        );
    }
    let ray = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::Z,
    };
    assert_eq!(
        runtime.cast_ray(&ray, f32::NAN, QueryFilter::default()),
        Err(PhysicsError::InvalidRay)
    );
}

#[test]
fn collider_debug_frame_is_optional_and_tracks_world_pose() {
    let mut runtime = PhysicsRuntime::default();
    let transform = Transform {
        translation: Vec3::new(2.0, 3.0, 4.0),
        scale: Vec3::splat(2.0),
        ..Transform::default()
    };
    let box_collider = Collider {
        shape: ColliderShape::Box(Vec3::splat(0.5)),
        ..Collider::default()
    };
    runtime
        .spawn_body(30, &transform, &RigidBody::default(), &box_collider)
        .expect("box collider should spawn");

    assert!(runtime.debug_frame().is_none());
    runtime.set_debug_enabled(true);
    let frame = runtime
        .debug_frame()
        .expect("debug frame should be enabled");
    assert_eq!(frame.frame_id, 0);
    assert_eq!(frame.lines.len(), 12);
    assert!(frame.lines.iter().any(|line| {
        line.start.abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1.0e-4)
            && line.end.abs_diff_eq(Vec3::new(1.0, 2.0, 5.0), 1.0e-4)
    }));
}

#[test]
fn debug_extraction_does_not_change_simulation_state() {
    let mut debug_runtime = PhysicsRuntime::default();
    let mut plain_runtime = PhysicsRuntime::default();
    for runtime in [&mut debug_runtime, &mut plain_runtime] {
        runtime
            .spawn_body(
                39,
                &Transform {
                    translation: Vec3::new(0.0, -0.5, 0.0),
                    ..Transform::default()
                },
                &RigidBody {
                    is_static: true,
                    ..RigidBody::default()
                },
                &Collider {
                    shape: ColliderShape::Box(Vec3::new(4.0, 0.5, 4.0)),
                    ..Collider::default()
                },
            )
            .expect("fixed floor should spawn");
        runtime
            .spawn_body(
                40,
                &Transform {
                    translation: Vec3::new(-1.0, 1.0, 0.0),
                    ..Transform::default()
                },
                &RigidBody::default(),
                &sphere(),
            )
            .expect("box should spawn");
        runtime
            .spawn_body(
                41,
                &Transform {
                    translation: Vec3::new(1.0, 1.4, 0.0),
                    ..Transform::default()
                },
                &RigidBody {
                    angular_velocity: Vec3::new(0.2, 0.7, 0.1),
                    ..RigidBody::default()
                },
                &Collider {
                    shape: ColliderShape::Box(Vec3::splat(0.4)),
                    ..Collider::default()
                },
            )
            .expect("sphere should spawn");
    }
    debug_runtime.set_debug_enabled(true);

    for step_index in 1..=60 {
        let frame_time = FrameTime {
            samples: vec![sample(step_index)],
            now: step_index as f32 / 60.0,
        };
        debug_runtime
            .step_frame(&frame_time)
            .expect("debug runtime should step");
        plain_runtime
            .step_frame(&frame_time)
            .expect("plain runtime should step");
        let _ = debug_runtime
            .debug_frame()
            .expect("enabled debug should extract");
    }
    assert_eq!(
        simulation_state_hash(&debug_runtime),
        simulation_state_hash(&plain_runtime),
        "debug extraction must not affect any body pose or velocity"
    );
    assert!(
        debug_runtime.debug_frame().unwrap().lines.len() > 12,
        "fixture should publish both box and sphere debug geometry"
    );
}

fn simulation_state_hash(runtime: &PhysicsRuntime) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update((runtime.entity_state.len() as u64).to_le_bytes());
    for (entity_bits, state) in &runtime.entity_state {
        let body = runtime
            .bodies
            .get(state.body_handle)
            .expect("mapped body should remain live");
        digest.update(entity_bits.to_le_bytes());
        let translation = body.translation();
        let rotation = body.rotation().quaternion();
        let linear_velocity = body.linvel();
        let angular_velocity = body.angvel();
        for value in [
            translation.x,
            translation.y,
            translation.z,
            rotation.i,
            rotation.j,
            rotation.k,
            rotation.w,
            linear_velocity.x,
            linear_velocity.y,
            linear_velocity.z,
            angular_velocity.x,
            angular_velocity.y,
            angular_velocity.z,
        ] {
            assert!(value.is_finite(), "simulation state must remain finite");
            digest.update(value.to_bits().to_le_bytes());
        }
    }
    digest.finalize().into()
}
