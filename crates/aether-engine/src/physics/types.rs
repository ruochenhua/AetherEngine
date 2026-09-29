//! Shared public types for the physics runtime and system adapter.

use thiserror::Error;

/// Errors returned while creating, synchronizing, or stepping physics state.
#[derive(Debug, Error, PartialEq)]
pub enum PhysicsError {
    /// A ray origin, direction, or maximum distance is invalid.
    #[error("physics ray must have a finite origin, non-zero finite direction, and positive finite length")]
    InvalidRay,
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

/// Closest collider hit returned by a physics ray query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsHit {
    /// Stable ECS entity identifier owning the hit collider.
    pub entity_bits: u64,
    /// Distance from the ray origin to the hit, in world units.
    pub toi: f32,
    /// World-space surface normal at the hit point.
    pub normal: glam::Vec3,
}

/// One colored world-space segment in a physics debug frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugLine {
    /// Segment start in world space.
    pub start: glam::Vec3,
    /// Segment end in world space.
    pub end: glam::Vec3,
    /// RGBA line color.
    pub color: [f32; 4],
}

/// Immutable collider wireframe snapshot for one physics simulation frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsDebugFrame {
    /// Last consumed fixed-step index when this snapshot was extracted.
    pub frame_id: u64,
    /// Collider wireframe segments in deterministic entity/collider order.
    pub lines: Vec<DebugLine>,
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

/// Cumulative count of Rapier handles created by one [`super::PhysicsRuntime`].
/// This is a runtime-scoped object count, not process heap bytes or allocator calls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PhysicsAllocationStats {
    /// Rigid-body handles created since this runtime was constructed.
    pub body_handles_created: u64,
    /// Collider handles created since this runtime was constructed.
    pub collider_handles_created: u64,
}
