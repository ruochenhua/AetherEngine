//! Deterministic CPU particle simulation and immutable render snapshots.

mod deterministic;
pub(crate) mod render;
mod runtime;
mod types;

pub use runtime::ParticleRuntime;
pub use types::{
    FrameStatus, ParticleEmitterConfig, ParticleError, ParticleFrame, ParticleRenderItem,
    StepOutcome, MAX_PARTICLES_GLOBAL, MAX_PARTICLES_PER_EMITTER, PARTICLE_FIXED_DT_SECONDS,
};
