//! Canonical simulation math, budget enforcement, and state hashing.

use super::runtime::{EmitterState, ParticleRuntime, ParticleState};
use super::types::{
    FrameStatus, ParticleEmitterConfig, ParticleError, ParticleFrame, ParticleRenderItem,
    MAX_PARTICLES_GLOBAL, MAX_PARTICLES_PER_EMITTER,
};
use glam::{Quat, Vec3};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn validate_config(config: &ParticleEmitterConfig) -> Result<(), ParticleError> {
    let id = config.entity_bits;
    let scalars = [
        config.emission_rate,
        config.lifetime[0],
        config.lifetime[1],
        config.initial_speed[0],
        config.initial_speed[1],
        config.start_size,
        config.end_size,
    ];
    if scalars.iter().any(|value| !value.is_finite())
        || config.position.iter().any(|value| !value.is_finite())
        || config.rotation.iter().any(|value| !value.is_finite())
        || config.gravity.iter().any(|value| !value.is_finite())
        || config.start_color.iter().any(|value| !value.is_finite())
        || config.end_color.iter().any(|value| !value.is_finite())
    {
        return Err(ParticleError::NonFinite { entity_bits: id });
    }
    if config.emission_rate < 0.0
        || config.max_particles == 0
        || config.max_particles > MAX_PARTICLES_PER_EMITTER
        || config.lifetime[0] <= 0.0
        || config.lifetime[1] < config.lifetime[0]
        || config.initial_speed[0] < 0.0
        || config.initial_speed[1] < config.initial_speed[0]
        || config.start_size < 0.0
        || config.end_size < 0.0
        || config
            .start_color
            .iter()
            .chain(config.end_color.iter())
            .any(|c| !(0.0..=1.0).contains(c))
    {
        return Err(ParticleError::InvalidConfig {
            entity_bits: id,
            reason: "range, rate, size, or color is outside the supported domain",
        });
    }
    let rotation = Quat::from_array(config.rotation);
    if !rotation.length_squared().is_finite() || rotation.length_squared() <= f32::EPSILON {
        return Err(ParticleError::InvalidConfig {
            entity_bits: id,
            reason: "rotation quaternion must be finite and non-zero",
        });
    }
    Ok(())
}

pub(super) fn update_particles(state: &mut EmitterState, dt: f32) -> Result<(), ParticleError> {
    let gravity = Vec3::from_array(state.config.gravity);
    for particle in &mut state.particles {
        particle.age += dt;
        particle.velocity += gravity * dt;
        particle.position += particle.velocity * dt;
        if !particle.age.is_finite()
            || !particle.position.is_finite()
            || !particle.velocity.is_finite()
        {
            return Err(ParticleError::NonFinite {
                entity_bits: state.config.entity_bits,
            });
        }
    }
    state
        .particles
        .retain(|particle| particle.age < particle.lifetime);
    Ok(())
}

pub(super) fn spawn_particle(config: &ParticleEmitterConfig, particle_id: u64) -> ParticleState {
    let random = splitmix64(config.seed ^ config.entity_bits.rotate_left(17) ^ particle_id);
    let unit = (random >> 40) as f32 / 16_777_216.0;
    let speed = lerp(config.initial_speed[0], config.initial_speed[1], unit);
    let rotation = (splitmix64(random) >> 40) as f32 / 16_777_216.0 * std::f32::consts::TAU;
    let orientation = Quat::from_array(config.rotation).normalize();
    let velocity = orientation * Vec3::Y * speed;
    let lifetime_unit = (splitmix64(random ^ 0x9e37_79b9_7f4a_7c15) >> 40) as f32 / 16_777_216.0;
    ParticleState {
        particle_id,
        age: 0.0,
        lifetime: lerp(config.lifetime[0], config.lifetime[1], lifetime_unit),
        position: Vec3::from_array(config.position),
        velocity,
        rotation,
    }
}

pub(super) fn trim_global_budget(emitters: &mut BTreeMap<u64, EmitterState>) -> u32 {
    let mut available = MAX_PARTICLES_GLOBAL as usize;
    let mut dropped = 0_u32;
    for state in emitters.values_mut() {
        if state.particles.len() > available {
            let removed = state.particles.len() - available;
            state.particles.truncate(available);
            dropped = dropped.saturating_add(u32::try_from(removed).unwrap_or(u32::MAX));
        }
        available = available.saturating_sub(state.particles.len());
    }
    dropped
}

pub(super) fn hash_state(runtime: &ParticleRuntime, items: &[ParticleRenderItem]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"AetherParticleStateV1");
    hash.update(runtime.scene_generation.to_le_bytes());
    hash.update(runtime.last_step_index.to_le_bytes());
    hash.update((runtime.emitters.len() as u64).to_le_bytes());
    for (entity, state) in &runtime.emitters {
        hash.update(entity.to_le_bytes());
        hash_config(&mut hash, &state.config);
        hash.update(state.spawn_counter.to_le_bytes());
        hash_f32(&mut hash, state.emission_remainder);
        hash.update([u8::from(state.burst_pending), u8::from(state.config.paused)]);
        hash.update((state.particles.len() as u64).to_le_bytes());
        for particle in &state.particles {
            hash.update(particle.particle_id.to_le_bytes());
            hash_f32(&mut hash, particle.age);
            hash_f32(&mut hash, particle.lifetime);
            for value in particle
                .position
                .to_array()
                .into_iter()
                .chain(particle.velocity.to_array())
            {
                hash_f32(&mut hash, value);
            }
            hash_f32(&mut hash, particle.rotation);
        }
    }
    hash.update((items.len() as u64).to_le_bytes());
    for item in items {
        hash.update(item.emitter_entity.to_le_bytes());
        hash.update(item.particle_id.to_le_bytes());
        for value in item
            .position
            .into_iter()
            .chain([item.size, item.rotation])
            .chain(item.color)
        {
            hash_f32(&mut hash, value);
        }
        hash.update([item.blend as u8]);
    }
    hash.finalize().into()
}

pub(super) fn empty_frame(frame_id: u64, scene_generation: u64) -> ParticleFrame {
    let mut hash = Sha256::new();
    hash.update(scene_generation.to_le_bytes());
    hash.update(frame_id.to_le_bytes());
    ParticleFrame {
        frame_id,
        scene_generation,
        items: Arc::from([]),
        dropped_count: 0,
        state_hash: hash.finalize().into(),
        status: FrameStatus::Empty,
    }
}

pub(super) fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn hash_config(hash: &mut Sha256, config: &ParticleEmitterConfig) {
    hash.update(config.emission_rate.to_bits().to_le_bytes());
    hash.update(config.burst.to_le_bytes());
    hash.update(config.max_particles.to_le_bytes());
    for value in config
        .lifetime
        .into_iter()
        .chain(config.initial_speed)
        .chain(config.position)
        .chain(config.rotation)
        .chain(config.gravity)
        .chain([config.start_size, config.end_size])
        .chain(config.start_color)
        .chain(config.end_color)
    {
        hash_f32(hash, value);
    }
    hash.update([config.blend as u8, u8::from(config.paused)]);
    hash.update(config.seed.to_le_bytes());
    hash.update(MAX_PARTICLES_PER_EMITTER.to_le_bytes());
    hash.update(MAX_PARTICLES_GLOBAL.to_le_bytes());
}

fn hash_f32(hash: &mut Sha256, value: f32) {
    hash.update(value.to_bits().to_le_bytes());
}

pub(super) fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}
