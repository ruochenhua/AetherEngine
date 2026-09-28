use crate::{ecs::World, time::FrameTime};

pub use super::runtime::PhysicsRuntime;
pub use super::types::{PhysicsError, StepStats};
use super::validation::{is_replay_frame, validate_samples};

/// Advances the physics runtime for the exact samples published for this frame.
///
/// Synchronizes ECS state, consumes the exact published samples, and writes
/// physics-authoritative transforms back to the world.
pub fn physics_system(
    runtime: &mut PhysicsRuntime,
    world: &mut World,
    frame_time: &FrameTime,
) -> Result<StepStats, PhysicsError> {
    if !is_replay_frame(
        &frame_time.samples,
        runtime.last_step_index,
        runtime.fixed_dt,
    ) {
        validate_samples(
            &frame_time.samples,
            runtime.last_step_index,
            runtime.fixed_dt,
        )?;
    }
    runtime.sync_in(world)?;
    if runtime.paused {
        return Ok(StepStats {
            active_bodies: runtime.bodies.len() as u32,
            ..StepStats::default()
        });
    }
    let stats = runtime.step_frame(frame_time)?;
    runtime.sync_out(world)?;
    Ok(stats)
}
