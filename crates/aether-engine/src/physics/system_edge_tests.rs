use super::super::{
    Collider, ColliderShape, PhysicsDesc, PhysicsError, PhysicsRuntime, RigidBody,
    TransformAuthority,
};
use crate::ecs::components::Transform;
use crate::time::{FrameTime, TimeSample};
use glam::Vec3;

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
fn invalid_shape_error_identifies_entity_and_shape() {
    let mut runtime = PhysicsRuntime::default();
    let invalid = Collider {
        shape: ColliderShape::Sphere(f32::NAN),
        ..Collider::default()
    };

    let error = runtime
        .spawn_body(42, &Transform::default(), &RigidBody::default(), &invalid)
        .expect_err("non-finite radius must be rejected");

    assert!(matches!(
        error,
        PhysicsError::InvalidShape {
            entity_bits: 42,
            ..
        }
    ));
    assert_eq!(runtime.body_count(), 0, "failed spawn must be atomic");
}

#[test]
fn paused_step_still_rejects_invalid_time_samples_without_mutation() {
    let mut runtime = PhysicsRuntime::default();
    runtime
        .spawn_body(44, &Transform::default(), &RigidBody::default(), &sphere())
        .expect("body should register");
    runtime.set_paused(true);
    let before = runtime.body_state(44);

    let error = runtime
        .step_frame(&FrameTime {
            samples: vec![TimeSample {
                time: f32::NAN,
                dt: f32::NAN,
                step_index: 2,
            }],
            now: f32::NAN,
        })
        .expect_err("paused runtime must still reject invalid published samples");

    assert!(matches!(
        error,
        PhysicsError::InvalidTimeSample {
            sample_index: 0,
            ..
        }
    ));
    assert_eq!(runtime.body_state(44), before);
    assert_eq!(
        runtime.step_frame(&FrameTime::default()).unwrap().substeps,
        0
    );
}

#[test]
fn non_uniform_scale_is_rejected_without_allocating_a_body() {
    let mut runtime = PhysicsRuntime::default();
    let transform = Transform {
        scale: Vec3::new(1.0, 2.0, 1.0),
        ..Transform::default()
    };

    let error = runtime
        .spawn_body(43, &transform, &RigidBody::default(), &sphere())
        .expect_err("non-uniform collider scale is unsupported");

    assert!(matches!(
        error,
        PhysicsError::InvalidTransform {
            entity_bits: 43,
            ..
        }
    ));
    assert_eq!(runtime.entity_count(), 0);
    assert_eq!(runtime.collider_count(), 0);
}

#[test]
fn stale_backend_handle_fails_without_partially_clearing_entity_maps() {
    let mut runtime = PhysicsRuntime::default();
    let desc = PhysicsDesc {
        body: RigidBody::default(),
        colliders: vec![sphere()],
        authority: TransformAuthority::Physics,
    };
    runtime
        .spawn_entity(72, &Transform::default(), &desc)
        .expect("body should register");
    let body_handle = runtime.entity_to_body[&72];
    runtime.bodies.remove(
        body_handle,
        &mut runtime.islands,
        &mut runtime.colliders,
        &mut runtime.impulse_joints,
        &mut runtime.multibody_joints,
        true,
    );

    let error = runtime
        .remove_entity(72)
        .expect_err("missing backend body must fail as stale");

    assert!(matches!(
        error,
        PhysicsError::StaleHandle { entity_bits: 72 }
    ));
    assert_eq!(runtime.entity_count(), 1);
    assert_eq!(runtime.body_count(), 1);
    assert_eq!(runtime.collider_count(), 1);
}

#[test]
fn ten_second_replay_is_bit_stable_across_fresh_runtimes() {
    fn simulate() -> (Vec3, Vec3) {
        let mut runtime = PhysicsRuntime::default();
        runtime
            .spawn_body(901, &Transform::default(), &RigidBody::default(), &sphere())
            .expect("valid replay body");
        runtime
            .step_frame(&FrameTime {
                samples: (1..=600).map(sample).collect(),
                now: 10.0,
            })
            .expect("ten second fixed replay");
        runtime.body_state(901).expect("replay body remains")
    }

    assert_eq!(simulate(), simulate());
}
