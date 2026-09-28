//! Shared public types for the physics runtime and system adapter.

use thiserror::Error;

/// Errors returned while creating, synchronizing, or stepping physics state.
#[derive(Debug, Error, PartialEq)]
pub enum PhysicsError {
    /// An entity already owns a physics body.
    #[error("entity {entity_bits} already has a physics body")]
    DuplicateEntity {
        /// Stable ECS identifier of the duplicate entity.
        entity_bits: u64,
    },
    /// A requested entity or one of its Rapier handles is no longer valid.
    #[error("physics handle for entity {entity_bits} is stale")]
    StaleHandle {
        /// Stable ECS identifier whose backend mapping is stale.
        entity_bits: u64,
    },
    /// A transform could not be represented safely by the physics backend.
    #[error("invalid transform for entity {entity_bits}: {reason}")]
    InvalidTransform {
        /// Stable ECS entity identifier.
        entity_bits: u64,
        /// Human-readable validation detail.
        reason: &'static str,
    },
    /// Rigid-body properties are invalid.
    #[error("invalid rigid body for entity {entity_bits}: {reason}")]
    InvalidRigidBody {
        /// Stable ECS entity identifier.
        entity_bits: u64,
        /// Human-readable validation detail.
        reason: &'static str,
    },
    /// A collider shape or material property is unsupported or invalid.
    #[error("invalid {shape} collider for entity {entity_bits}: {reason}")]
    InvalidShape {
        /// Stable ECS entity identifier.
        entity_bits: u64,
        /// Name of the attempted shape.
        shape: &'static str,
        /// Human-readable validation detail.
        reason: &'static str,
    },
    /// The frame's fixed-step samples violate the time-control contract.
    #[error("invalid physics time sample {sample_index}: {reason}")]
    InvalidTimeSample {
        /// Zero-based offset within the submitted frame samples.
        sample_index: usize,
        /// Human-readable validation detail.
        reason: &'static str,
    },
}

/// Result summary for the most recent published frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepStats {
    /// Number of fixed-time samples simulated.
    pub substeps: u32,
    /// Total simulated duration in seconds.
    pub simulated_time: f32,
    /// Number of bodies in the runtime after stepping.
    pub active_bodies: u32,
}
