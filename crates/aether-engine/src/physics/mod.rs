//! Rapier-backed rigid-body simulation core.
pub mod components;
mod lifecycle;
mod runtime;
pub mod system;
mod types;
mod validation;
pub use components::{
    Collider, ColliderList, ColliderShape, PhysicsDesc, RigidBody, TransformAuthority,
};
pub use runtime::PhysicsRuntime;
pub use system::physics_system;
pub use types::{PhysicsError, StepStats};

#[cfg(test)]
mod system_tests;
