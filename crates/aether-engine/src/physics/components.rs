//! Components accepted by the physics runtime.

#[path = "components/core.rs"]
mod core;
#[path = "components/lifecycle.rs"]
mod lifecycle;

pub use core::{Collider, ColliderShape, RigidBody};
pub use lifecycle::{ColliderList, PhysicsDesc, TransformAuthority};
