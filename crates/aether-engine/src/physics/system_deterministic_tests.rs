use super::{Collider, ColliderShape, PhysicsRuntime, RigidBody};
use crate::ecs::components::Transform;
use crate::time::{FrameTime, TimeSample};
use glam::Vec3;
use sha2::{Digest, Sha256};

fn replay_hash() -> ([u8; 32], usize, crate::physics::PhysicsAllocationStats) {
    let mut runtime = PhysicsRuntime::default();
    let fixed = RigidBody {
        is_static: true,
        ..RigidBody::default()
    };
    runtime
        .spawn_body(
            1,
            &Transform {
                translation: Vec3::new(0.0, -0.5, 0.0),
                ..Transform::default()
            },
            &fixed,
            &Collider {
                shape: ColliderShape::Box(Vec3::new(8.0, 0.5, 8.0)),
                ..Collider::default()
            },
        )
        .unwrap();

    for index in 0..100_u64 {
        let is_box = index % 2 == 0;
        let y = 0.5 + (index / 10) as f32 * 1.05;
        let x = ((index % 10) as f32 - 4.5) * 1.05;
        let collider = Collider {
            shape: if is_box {
                ColliderShape::Box(Vec3::splat(0.5))
            } else {
                ColliderShape::Sphere(0.5)
            },
            friction: 0.7,
            ..Collider::default()
        };
        runtime
            .spawn_body(
                index + 2,
                &Transform {
                    translation: Vec3::new(x, y, 0.0),
                    ..Transform::default()
                },
                &RigidBody::default(),
                &collider,
            )
            .unwrap();
    }

    let samples = (1..=600)
        .map(|step_index| TimeSample {
            time: step_index as f32 / 60.0,
            dt: 1.0 / 60.0,
            step_index,
        })
        .collect();
    runtime
        .step_frame(&FrameTime { samples, now: 10.0 })
        .unwrap();

    let (resting_position, resting_velocity) = runtime.body_state(101).unwrap();
    assert!(
        (9.0..9.8).contains(&resting_position.y),
        "top stack body should settle after contact, got {resting_position:?}"
    );
    assert!(resting_velocity.length() < 0.1);

    let mut digest = Sha256::new();
    for entity_bits in 1..=101_u64 {
        let (position, velocity) = runtime.body_state(entity_bits).unwrap();
        for value in position.to_array().into_iter().chain(velocity.to_array()) {
            assert!(value.is_finite(), "body {entity_bits} has non-finite state");
            digest.update(value.to_bits().to_le_bytes());
        }
    }
    (
        digest.finalize().into(),
        runtime.body_count() - 1,
        runtime.allocation_stats(),
    )
}

#[test]
fn hundred_body_ten_second_replay_has_stable_state_hash() {
    let first = replay_hash();
    let second = replay_hash();
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, 100, "dynamic body count excludes the fixed floor");
    assert_eq!(
        first.2.body_handles_created, 101,
        "runtime should create 100 dynamic bodies and one fixed floor"
    );
    assert_eq!(first.2.collider_handles_created, 101);
}
