//! Fixed-step CPU particle runtime. It consumes time samples and never queries ECS.

use super::deterministic::{
    empty_frame, hash_state, lerp, spawn_particle, trim_global_budget, update_particles,
    validate_config,
};
use super::types::{
    FrameStatus, ParticleEmitterConfig, ParticleError, ParticleFrame, ParticleRenderItem,
    StepOutcome, PARTICLE_FIXED_DT_SECONDS,
};
use crate::time::{DeterministicSystem, FrameTime, TimeSample};
use glam::Vec3;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ParticleState {
    pub(super) particle_id: u64,
    pub(super) age: f32,
    pub(super) lifetime: f32,
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) rotation: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct EmitterState {
    pub(super) config: ParticleEmitterConfig,
    pub(super) particles: Vec<ParticleState>,
    pub(super) spawn_counter: u64,
    pub(super) emission_remainder: f32,
    pub(super) burst_pending: bool,
}

impl EmitterState {
    fn new(config: ParticleEmitterConfig) -> Self {
        Self {
            config,
            particles: Vec::new(),
            spawn_counter: 0,
            emission_remainder: 0.0,
            burst_pending: true,
        }
    }
}

/// CPU-side particle runtime with transactional updates and stable budgets.
#[derive(Clone, Debug)]
pub struct ParticleRuntime {
    pub(super) emitters: BTreeMap<u64, EmitterState>,
    pub(super) current: Arc<ParticleFrame>,
    pub(super) scene_generation: u64,
    pub(super) frame_id: u64,
    pub(super) last_step_index: u64,
    pub(super) pending_dropped: u32,
}

impl Default for ParticleRuntime {
    fn default() -> Self {
        Self {
            emitters: BTreeMap::new(),
            current: Arc::new(empty_frame(0, 0)),
            scene_generation: 0,
            frame_id: 0,
            last_step_index: 0,
            pending_dropped: 0,
        }
    }
}

impl ParticleRuntime {
    /// Returns the current immutable snapshot.
    pub fn current_frame(&self) -> &Arc<ParticleFrame> {
        &self.current
    }

    /// Returns the number of configured emitters.
    pub fn emitter_count(&self) -> usize {
        self.emitters.len()
    }

    /// Replaces emitter configuration atomically, preserving state for unchanged entries.
    pub fn configure_emitters(
        &mut self,
        configs: Vec<ParticleEmitterConfig>,
    ) -> Result<(), ParticleError> {
        let mut next = BTreeMap::new();
        let mut previous = self.emitters.clone();
        for config in configs {
            validate_config(&config)?;
            let id = config.entity_bits;
            if next.contains_key(&id) {
                return Err(ParticleError::InvalidConfig {
                    entity_bits: id,
                    reason: "duplicate emitter entity bits",
                });
            }
            let state = match previous.remove(&id) {
                Some(mut existing) => {
                    existing.config = config;
                    existing
                }
                None => EmitterState::new(config),
            };
            next.insert(id, state);
        }
        let removed = !previous.is_empty();
        self.emitters = next;
        if removed {
            self.invalidate_generation();
        }
        Ok(())
    }

    /// Restarts one emitter, increments scene generation, and publishes an empty frame.
    pub fn restart_emitter(&mut self, entity_bits: u64) -> Result<(), ParticleError> {
        let state = self
            .emitters
            .get(&entity_bits)
            .ok_or(ParticleError::MissingEmitter { entity_bits })?;
        let config = state.config.clone();
        self.emitters.insert(entity_bits, EmitterState::new(config));
        self.invalidate_generation();
        Ok(())
    }

    /// Advances all configured emitters using only fixed time samples.
    pub fn simulate(&mut self, frame_time: &FrameTime) -> StepOutcome {
        let mut candidate = self.clone();
        match candidate.simulate_candidate(&frame_time.samples) {
            Ok(dropped_count) => {
                candidate.frame_id = candidate.frame_id.saturating_add(1);
                candidate.pending_dropped = dropped_count;
                let frame = Arc::new(candidate.snapshot(dropped_count));
                candidate.current = Arc::clone(&frame);
                *self = candidate;
                if frame.items.is_empty() {
                    StepOutcome::Empty(frame)
                } else {
                    StepOutcome::Published(frame)
                }
            }
            Err(error) => StepOutcome::ReusedPrevious {
                frame: Arc::clone(&self.current),
                error,
            },
        }
    }

    /// Changes scene generation and invalidates all emitter simulation state.
    pub fn reset_generation(&mut self, generation: u64) {
        self.scene_generation = generation.max(self.scene_generation.saturating_add(1));
        for state in self.emitters.values_mut() {
            let config = state.config.clone();
            *state = EmitterState::new(config);
        }
        self.last_step_index = 0;
        self.pending_dropped = 0;
        self.publish_empty();
    }

    fn simulate_candidate(&mut self, samples: &[TimeSample]) -> Result<u32, ParticleError> {
        let mut dropped = 0_u32;
        for sample in samples {
            if !sample.time.is_finite()
                || sample.time < 0.0
                || !sample.dt.is_finite()
                || sample.dt <= 0.0
            {
                return Err(ParticleError::NonFinite { entity_bits: 0 });
            }
            if sample.dt.to_bits() != PARTICLE_FIXED_DT_SECONDS.to_bits() {
                return Err(ParticleError::InvalidConfig {
                    entity_bits: 0,
                    reason: "particle samples must use the fixed 60 Hz time step",
                });
            }
            if sample.step_index != self.last_step_index.saturating_add(1) {
                return Err(ParticleError::InvalidConfig {
                    entity_bits: 0,
                    reason: "time samples must have contiguous step indices",
                });
            }
            if sample.time.to_bits()
                != (sample.step_index as f32 * PARTICLE_FIXED_DT_SECONDS).to_bits()
            {
                return Err(ParticleError::InvalidConfig {
                    entity_bits: 0,
                    reason: "sample time must equal step_index multiplied by fixed_dt",
                });
            }
            for state in self.emitters.values_mut() {
                if state.config.paused {
                    continue;
                }
                update_particles(state, sample.dt)?;
                let mut requested = if state.burst_pending {
                    u64::from(state.config.burst)
                } else {
                    0
                };
                state.burst_pending = false;
                let emitted_fraction = state.config.emission_rate * sample.dt;
                let accumulated = state.emission_remainder + emitted_fraction;
                if !emitted_fraction.is_finite() || !accumulated.is_finite() {
                    return Err(ParticleError::BudgetOverflow {
                        entity_bits: state.config.entity_bits,
                    });
                }
                if accumulated >= u64::MAX as f32 {
                    return Err(ParticleError::BudgetOverflow {
                        entity_bits: state.config.entity_bits,
                    });
                }
                let rate_count = accumulated.floor() as u64;
                state.emission_remainder = accumulated - rate_count as f32;
                requested =
                    requested
                        .checked_add(rate_count)
                        .ok_or(ParticleError::BudgetOverflow {
                            entity_bits: state.config.entity_bits,
                        })?;
                let capacity = usize::try_from(state.config.max_particles)
                    .unwrap_or(usize::MAX)
                    .saturating_sub(state.particles.len());
                let admitted = requested.min(capacity as u64);
                let first_id = state.spawn_counter;
                state.spawn_counter = state.spawn_counter.checked_add(requested).ok_or(
                    ParticleError::BudgetOverflow {
                        entity_bits: state.config.entity_bits,
                    },
                )?;
                dropped =
                    dropped.saturating_add(u32::try_from(requested - admitted).unwrap_or(u32::MAX));
                for offset in 0..admitted {
                    let particle_id = first_id.saturating_add(offset);
                    state
                        .particles
                        .push(spawn_particle(&state.config, particle_id));
                }
            }
            dropped = dropped.saturating_add(trim_global_budget(&mut self.emitters));
            self.last_step_index = sample.step_index;
        }
        Ok(dropped)
    }

    fn snapshot(&self, dropped_count: u32) -> ParticleFrame {
        let mut items = Vec::new();
        for state in self.emitters.values() {
            for particle in &state.particles {
                let t = (particle.age / particle.lifetime).clamp(0.0, 1.0);
                items.push(ParticleRenderItem {
                    emitter_entity: state.config.entity_bits,
                    particle_id: particle.particle_id,
                    position: particle.position.to_array(),
                    size: lerp(state.config.start_size, state.config.end_size, t),
                    rotation: particle.rotation,
                    color: std::array::from_fn(|i| {
                        lerp(state.config.start_color[i], state.config.end_color[i], t)
                    }),
                    texture: None,
                    blend: state.config.blend,
                });
            }
        }
        let state_hash = hash_state(self, &items);
        ParticleFrame {
            frame_id: self.frame_id,
            scene_generation: self.scene_generation,
            status: if items.is_empty() {
                FrameStatus::Empty
            } else {
                FrameStatus::Ready
            },
            items: Arc::from(items),
            dropped_count,
            state_hash,
        }
    }

    fn invalidate_generation(&mut self) {
        self.scene_generation = self.scene_generation.saturating_add(1);
        self.last_step_index = 0;
        self.pending_dropped = 0;
        self.publish_empty();
    }

    fn publish_empty(&mut self) {
        self.frame_id = self.frame_id.saturating_add(1);
        self.current = Arc::new(empty_frame(self.frame_id, self.scene_generation));
    }
}

impl DeterministicSystem for ParticleRuntime {
    type Checkpoint = Self;
    type Error = ParticleError;

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        *self = checkpoint;
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.invalidate_generation();
        for state in self.emitters.values_mut() {
            let config = state.config.clone();
            *state = EmitterState::new(config);
        }
        Ok(())
    }

    fn step(&mut self, sample: TimeSample) -> Result<(), Self::Error> {
        self.simulate_candidate(&[sample]).map(|dropped| {
            self.pending_dropped = self.pending_dropped.saturating_add(dropped);
        })
    }

    fn dispatch(&mut self, _frame: &FrameTime) -> Result<(), Self::Error> {
        self.frame_id = self.frame_id.saturating_add(1);
        self.current = Arc::new(self.snapshot(self.pending_dropped));
        self.pending_dropped = 0;
        Ok(())
    }
}
