use super::components::{Collider, ColliderShape, PhysicsDesc, RigidBody};
use super::system::PhysicsError;
use crate::ecs::components::Transform;
use crate::time::TimeSample;
use glam::Vec3;
use rapier3d::na::Vector3;
use rapier3d::prelude::ColliderBuilder;

pub(super) fn validate_transform(
    entity_bits: u64,
    transform: &Transform,
) -> Result<(), PhysicsError> {
    if !transform.translation.is_finite() {
        return Err(invalid_transform(entity_bits, "translation must be finite"));
    }
    if !transform.rotation.is_finite() || transform.rotation.length_squared() <= f32::EPSILON {
        return Err(invalid_transform(
            entity_bits,
            "rotation must be finite and non-zero",
        ));
    }
    if !transform.scale.is_finite()
        || transform.scale.min_element() <= 0.0
        || (transform.scale.x - transform.scale.y).abs() > 1.0e-5
        || (transform.scale.x - transform.scale.z).abs() > 1.0e-5
    {
        return Err(invalid_transform(
            entity_bits,
            "scale must be positive, finite, and uniform",
        ));
    }
    Ok(())
}

pub(super) fn validate_body(entity_bits: u64, body: &RigidBody) -> Result<(), PhysicsError> {
    if !body.velocity.is_finite() || !body.angular_velocity.is_finite() {
        return Err(PhysicsError::InvalidRigidBody {
            entity_bits,
            reason: "velocities must be finite",
        });
    }
    if !body.mass.is_finite() || (!body.is_static && body.mass <= 0.0) {
        return Err(PhysicsError::InvalidRigidBody {
            entity_bits,
            reason: "dynamic body mass must be finite and positive",
        });
    }
    Ok(())
}

pub(super) fn validate_desc(
    entity_bits: u64,
    transform: &Transform,
    desc: &PhysicsDesc,
) -> Result<(), PhysicsError> {
    validate_transform(entity_bits, transform)?;
    validate_body(entity_bits, &desc.body)?;
    if desc.colliders.is_empty() || desc.colliders.len() > 4 {
        return Err(invalid_shape(
            entity_bits,
            "collider set",
            "an entity must have between one and four colliders",
        ));
    }
    for collider in &desc.colliders {
        let _ = collider_builder(entity_bits, transform.scale, collider)?;
    }
    Ok(())
}

pub(super) fn collider_builder(
    entity_bits: u64,
    scale: Vec3,
    collider: &Collider,
) -> Result<ColliderBuilder, PhysicsError> {
    let shape = match collider.shape {
        ColliderShape::Sphere(radius) if radius.is_finite() && radius > 0.0 => {
            ColliderBuilder::ball(radius * scale.x)
        }
        ColliderShape::Sphere(_) => {
            return Err(invalid_shape(
                entity_bits,
                "sphere",
                "radius must be finite and positive",
            ));
        }
        ColliderShape::Box(half_extents)
            if half_extents.is_finite() && half_extents.min_element() > 0.0 =>
        {
            ColliderBuilder::cuboid(
                half_extents.x * scale.x,
                half_extents.y * scale.y,
                half_extents.z * scale.z,
            )
        }
        ColliderShape::Box(_) => {
            return Err(invalid_shape(
                entity_bits,
                "box",
                "half extents must be finite and positive",
            ));
        }
        ColliderShape::Capsule(_, _) => {
            return Err(invalid_shape(
                entity_bits,
                "capsule",
                "capsule colliders are outside the physics core",
            ));
        }
        ColliderShape::Mesh => {
            return Err(invalid_shape(
                entity_bits,
                "mesh",
                "mesh colliders are outside the physics core",
            ));
        }
    };
    if !collider.friction.is_finite() || collider.friction < 0.0 {
        return Err(invalid_shape(
            entity_bits,
            collider_name(&collider.shape),
            "friction must be finite and non-negative",
        ));
    }
    if !collider.restitution.is_finite() || !(0.0..=1.0).contains(&collider.restitution) {
        return Err(invalid_shape(
            entity_bits,
            collider_name(&collider.shape),
            "restitution must be finite and in [0, 1]",
        ));
    }
    Ok(shape
        .friction(collider.friction)
        .restitution(collider.restitution)
        // Dynamic-body creation rescales unit-density collider mass properties
        // to preserve the body's requested total mass and derive its inertia.
        .density(1.0)
        .sensor(collider.is_trigger))
}

pub(super) fn validate_samples(
    samples: &[TimeSample],
    last_step_index: u64,
    fixed_dt: Option<f32>,
) -> Result<(), PhysicsError> {
    for (index, sample) in samples.iter().enumerate() {
        if !sample.dt.is_finite() || sample.dt <= 0.0 || sample.dt > 0.1 {
            return Err(invalid_sample(index, "dt must be finite and in (0, 0.1]"));
        }
        if !sample.time.is_finite() {
            return Err(invalid_sample(index, "sample time must be finite"));
        }
        if fixed_dt.is_some_and(|dt| (sample.dt - dt).abs() > f32::EPSILON) {
            return Err(invalid_sample(
                index,
                "fixed dt changed within this runtime",
            ));
        }
        let expected = last_step_index.saturating_add(index as u64 + 1);
        if sample.step_index != expected {
            return Err(invalid_sample(index, "step indices must be contiguous"));
        }
    }
    Ok(())
}

pub(super) fn is_replay_frame(
    samples: &[TimeSample],
    last_step_index: u64,
    fixed_dt: Option<f32>,
) -> bool {
    samples.first().is_some_and(|sample| sample.step_index == 1)
        && samples
            .last()
            .is_some_and(|sample| sample.step_index == last_step_index)
        && validate_samples(samples, 0, fixed_dt).is_ok()
}

pub(super) fn vector3(value: Vec3) -> Vector3<f32> {
    Vector3::new(value.x, value.y, value.z)
}

fn invalid_sample(sample_index: usize, reason: &'static str) -> PhysicsError {
    PhysicsError::InvalidTimeSample {
        sample_index,
        reason,
    }
}

fn invalid_transform(entity_bits: u64, reason: &'static str) -> PhysicsError {
    PhysicsError::InvalidTransform {
        entity_bits,
        reason,
    }
}

fn invalid_shape(entity_bits: u64, shape: &'static str, reason: &'static str) -> PhysicsError {
    PhysicsError::InvalidShape {
        entity_bits,
        shape,
        reason,
    }
}

fn collider_name(shape: &ColliderShape) -> &'static str {
    match shape {
        ColliderShape::Sphere(_) => "sphere",
        ColliderShape::Box(_) => "box",
        ColliderShape::Capsule(_, _) => "capsule",
        ColliderShape::Mesh => "mesh",
    }
}
