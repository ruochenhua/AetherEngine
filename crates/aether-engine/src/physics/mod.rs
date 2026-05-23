//! Physics module (reserved for Phase 4).
//!
//! Currently provides empty ECS components for physics data.
//! The actual simulation system will be implemented in a future phase.
//!
//! ## Design
//!
//! - Phase 1-3: Components exist but have no simulation
//! - Phase 4: Implement `physics_system` with rapier3d or custom solver

pub mod components;
pub mod system;

pub use components::{Collider, ColliderShape, RigidBody};
pub use system::physics_system;
