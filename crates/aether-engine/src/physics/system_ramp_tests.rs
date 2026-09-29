use super::*;

#[test]
fn dynamic_sphere_rolls_down_a_frictional_ramp() {
    let mut runtime = PhysicsRuntime::default();
    let ramp_transform = Transform {
        translation: Vec3::new(0.0, 1.0, 0.0),
        rotation: glam::Quat::from_rotation_z(-20.0_f32.to_radians()),
        ..Transform::default()
    };
    let ramp = RigidBody {
        is_static: true,
        ..RigidBody::default()
    };
    let ramp_collider = Collider {
        shape: ColliderShape::Box(Vec3::new(2.5, 0.275, 0.5)),
        friction: 0.65,
        ..Collider::default()
    };
    runtime
        .spawn_body(1, &ramp_transform, &ramp, &ramp_collider)
        .expect("ramp should spawn");

    let sphere_transform = Transform {
        translation: Vec3::new(-1.5, 2.1, 0.0),
        scale: Vec3::splat(0.7),
        ..Transform::default()
    };
    let sphere_collider = Collider {
        shape: ColliderShape::Sphere(0.45),
        friction: 0.35,
        ..Collider::default()
    };
    runtime
        .spawn_body(
            2,
            &sphere_transform,
            &RigidBody::default(),
            &sphere_collider,
        )
        .expect("sphere should spawn");

    let samples = (1..=120).map(sample).collect::<Vec<_>>();
    runtime
        .step_frame(&FrameTime { samples, now: 2.0 })
        .expect("ramp simulation should step");

    let (position, _) = runtime.body_state(2).expect("sphere should exist");
    let handle = runtime.entity_to_body[&2];
    let body = &runtime.bodies[handle];
    assert!((body.mass() - RigidBody::default().mass).abs() < 1.0e-4);
    assert!(
        position.x > -1.0,
        "sphere should roll down ramp: {position:?}"
    );
    assert!(
        body.angvel().norm() > 0.1,
        "rolling should produce angular velocity: {:?}",
        body.angvel()
    );
}
