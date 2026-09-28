use super::core::{Collider, RigidBody};

/// Multiple colliders attached to a single entity's rigid body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColliderList(pub Vec<Collider>);

/// Selects which side owns a body's transform between simulation steps.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransformAuthority {
    /// Scene transforms drive the physics pose (fixed or externally controlled bodies).
    Scene,
    /// Physics drives the scene transform after stepping (dynamic bodies).
    #[default]
    Physics,
}

/// Physics description for one entity, validated when registered.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsDesc {
    /// Rigid-body behavior and initial velocities.
    pub body: RigidBody,
    /// Shapes attached to the body; the runtime accepts one through four.
    pub colliders: Vec<Collider>,
    /// Transform synchronization policy.
    pub authority: TransformAuthority,
}
