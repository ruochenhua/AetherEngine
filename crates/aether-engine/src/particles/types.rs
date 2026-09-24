//! Public particle configuration, frame, item, and result types.

use crate::asset::{texture::CpuTexture, Handle};
use crate::renderer::transparent::TransparentBlendMode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Maximum active particles retained by one emitter.
pub const MAX_PARTICLES_PER_EMITTER: u32 = 4096;
/// Maximum active particles retained by the runtime.
pub const MAX_PARTICLES_GLOBAL: u32 = 16_384;
/// Fixed particle simulation step in seconds.
pub const PARTICLE_FIXED_DT_SECONDS: f32 = 1.0 / 60.0;

/// Emitter configuration. Distances are meters; rates and lifetime use seconds.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ParticleEmitterConfig {
    /// Stable ECS entity bits, used only as an ordering key.
    #[serde(skip)]
    pub entity_bits: u64,
    /// Number of particles emitted per second.
    pub emission_rate: f32,
    /// Number emitted once after activation or restart.
    pub burst: u32,
    /// Maximum active particles retained for this emitter, at most 4096.
    pub max_particles: u32,
    /// Inclusive minimum and maximum particle lifetime in seconds.
    pub lifetime: [f32; 2],
    /// Inclusive minimum and maximum initial speed in meters per second.
    pub initial_speed: [f32; 2],
    /// Emitter position in world meters.
    pub position: [f32; 3],
    /// Emitter orientation quaternion in XYZW order.
    pub rotation: [f32; 4],
    /// World-space acceleration in meters per second squared.
    pub gravity: [f32; 3],
    /// Particle size at birth in meters.
    pub start_size: f32,
    /// Particle size at the end of its lifetime in meters.
    pub end_size: f32,
    /// Linear RGBA particle color at birth.
    pub start_color: [f32; 4],
    /// Linear RGBA particle color at the end of its lifetime.
    pub end_color: [f32; 4],
    /// Alpha or additive blend mode reused by the transparent renderer.
    pub blend: TransparentBlendMode,
    /// Stable user-provided random seed.
    pub seed: u64,
    /// Paused emitters neither age particles nor emit new ones.
    pub paused: bool,
}

impl Default for ParticleEmitterConfig {
    fn default() -> Self {
        Self {
            entity_bits: 0,
            emission_rate: 0.0,
            burst: 0,
            max_particles: MAX_PARTICLES_PER_EMITTER,
            lifetime: [1.0, 1.0],
            initial_speed: [1.0, 1.0],
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            gravity: [0.0, -9.81, 0.0],
            start_size: 0.1,
            end_size: 0.0,
            start_color: [1.0; 4],
            end_color: [1.0, 1.0, 1.0, 0.0],
            blend: TransparentBlendMode::Alpha,
            seed: 0,
            paused: false,
        }
    }
}

/// One CPU-simulated particle prepared for a later renderer slice.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleRenderItem {
    /// Stable emitter entity ordering key.
    pub emitter_entity: u64,
    /// Monotonic spawn id, reset only by emitter restart.
    pub particle_id: u64,
    /// World-space center in meters.
    pub position: [f32; 3],
    /// Interpolated size in meters.
    pub size: f32,
    /// Deterministic billboard rotation in radians.
    pub rotation: f32,
    /// Interpolated linear RGBA color.
    pub color: [f32; 4],
    /// Optional typed albedo texture; unresolved handles use the white fallback.
    pub texture: Option<Handle<CpuTexture>>,
    /// Transparent blend mode selected by the emitter.
    pub blend: TransparentBlendMode,
}

/// State of a successfully published particle frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameStatus {
    /// The frame contains at least one render item.
    Ready,
    /// No particles are alive in the frame.
    Empty,
}

/// Immutable, atomically published particle simulation snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleFrame {
    /// Monotonic publication id within the runtime.
    pub frame_id: u64,
    /// Scene generation; stale frames from previous generations must be ignored.
    pub scene_generation: u64,
    /// Items ordered by `(emitter_entity, particle_id)`.
    pub items: Arc<[ParticleRenderItem]>,
    /// Number of particles deterministically discarded by capacity limits this frame.
    pub dropped_count: u32,
    /// SHA-256 hash of canonical little-endian simulation state.
    pub state_hash: [u8; 32],
    /// Whether the published frame contains particles.
    pub status: FrameStatus,
}

/// Particle runtime failure. Failed simulation retains the current frame.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ParticleError {
    /// A configuration or simulation sample contains a non-finite value.
    #[error("particle runtime received non-finite data for emitter {entity_bits}")]
    NonFinite {
        /// Emitter containing invalid data, or zero for global time data.
        entity_bits: u64,
    },
    /// An emitter configuration violates a range or capacity contract.
    #[error("invalid particle configuration for emitter {entity_bits}: {reason}")]
    InvalidConfig {
        /// Emitter whose configuration or time sequence is invalid.
        entity_bits: u64,
        /// Stable explanation of the rejected contract.
        reason: &'static str,
    },
    /// A requested emitter does not exist.
    #[error("particle emitter {entity_bits} does not exist")]
    MissingEmitter {
        /// Requested stable emitter key.
        entity_bits: u64,
    },
    /// Arithmetic overflow prevents deterministic emission accounting.
    #[error("particle emission budget arithmetic overflow for emitter {entity_bits}")]
    BudgetOverflow {
        /// Emitter whose emission arithmetic overflowed.
        entity_bits: u64,
    },
}

/// Result of a particle simulation and publication attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum StepOutcome {
    /// A non-empty frame was atomically published.
    Published(Arc<ParticleFrame>),
    /// Simulation failed; the last complete frame remains current.
    ReusedPrevious {
        /// The unchanged frame that remains current.
        frame: Arc<ParticleFrame>,
        /// Cause of the rejected update.
        error: ParticleError,
    },
    /// An empty frame was atomically published.
    Empty(Arc<ParticleFrame>),
}
