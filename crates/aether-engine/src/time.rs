//! Deterministic frame-time generation with transactional system dispatch.
//!
//! The generic [`DeterministicSystem`] seam is the integration point for the
//! concrete particle, physics, and animation coordinators planned in T5/T6/T7.

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use thiserror::Error;

/// Selects how simulation samples are generated.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeMode {
    /// Accumulate wall time and dispatch bounded fixed substeps.
    #[default]
    WallClock,
    /// Dispatch exactly one fixed step per rendered frame.
    FixedStep,
    /// Reset and replay deterministic steps to a requested time.
    Seek,
}

/// One deterministic simulation step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeSample {
    /// Normalized simulation time after this step.
    pub time: f32,
    /// Fixed step duration.
    pub dt: f32,
    /// One-based deterministic step index.
    pub step_index: u64,
}

/// Samples published for one rendered frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameTime {
    /// Ordered samples actually dispatched during this frame.
    pub samples: Vec<TimeSample>,
    /// Last dispatched sample time, or zero when no samples were dispatched.
    pub now: f32,
}

/// Deterministic time generator configuration and mutable state.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeControl {
    /// Active generation mode.
    pub mode: TimeMode,
    /// Current normalized simulation time.
    pub simulation_time: f32,
    /// Duration of every deterministic step.
    pub fixed_dt: f32,
    /// Unconsumed fractional wall time.
    pub accumulator: f32,
    /// Configured wall-clock substep cap, additionally hard-capped at four.
    pub max_substeps: u32,
    /// Maximum number of steps accepted by seek.
    pub max_seek_steps: u32,
    /// Current deterministic step index.
    pub frame_index: u64,
    published: FrameTime,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::new(TimeMode::WallClock, 0.0, 1.0 / 60.0, 4, 4096)
            .unwrap_or_else(|_| unreachable!("static default time configuration is valid"))
    }
}

impl TimeControl {
    /// Constructs and validates a time control configuration.
    pub fn new(
        mode: TimeMode,
        simulation_time: f32,
        fixed_dt: f32,
        max_substeps: u32,
        max_seek_steps: u32,
    ) -> Result<Self, TimeError> {
        validate_config(simulation_time, fixed_dt, max_substeps, max_seek_steps)?;
        let control = Self {
            mode,
            simulation_time,
            fixed_dt,
            accumulator: 0.0,
            max_substeps,
            max_seek_steps,
            frame_index: 0,
            published: FrameTime::default(),
        };
        if mode == TimeMode::Seek {
            control.normalize_seek(simulation_time)?;
        }
        Ok(control)
    }

    /// Returns the last successfully published frame.
    pub fn published_frame(&self) -> &FrameTime {
        &self.published
    }

    /// Generates and publishes samples for the configured non-seek mode.
    pub fn advance_frame(&mut self, wall_dt: f32) -> Result<FrameTime, TimeError> {
        let mut next = self.clone();
        let frame = next.advance_candidate(wall_dt)?;
        next.published = frame.clone();
        *self = next;
        Ok(frame)
    }

    /// Resets time and publishes the full deterministic replay to `target`.
    pub fn seek_frame(&mut self, target: f32) -> Result<FrameTime, TimeError> {
        let mut next = self.clone();
        let frame = next.seek_candidate(target)?;
        next.published = frame.clone();
        *self = next;
        Ok(frame)
    }

    /// Advances, steps, and dispatches atomically against a deterministic system.
    pub fn advance_and_step<S: DeterministicSystem>(
        &mut self,
        wall_dt: f32,
        system: &mut S,
    ) -> Result<FrameTime, TimeError> {
        self.transaction(system, false, |next| next.advance_candidate(wall_dt))
    }

    /// Resets, replays, and dispatches a seek atomically.
    pub fn seek_and_step<S: DeterministicSystem>(
        &mut self,
        target: f32,
        system: &mut S,
    ) -> Result<FrameTime, TimeError> {
        self.transaction(system, true, |next| next.seek_candidate(target))
    }

    fn transaction<S, F>(
        &mut self,
        system: &mut S,
        reset: bool,
        generate: F,
    ) -> Result<FrameTime, TimeError>
    where
        S: DeterministicSystem,
        F: FnOnce(&mut Self) -> Result<FrameTime, TimeError>,
    {
        let control_checkpoint = self.clone();
        let system_checkpoint = system.checkpoint();
        let frame = generate(self)?;

        if reset {
            if let Err(cause) = system.reset() {
                return rollback(
                    self,
                    control_checkpoint,
                    system,
                    system_checkpoint,
                    SimulationStage::Reset,
                    0,
                    cause,
                );
            }
        }
        for sample in &frame.samples {
            if let Err(cause) = system.step(*sample) {
                return rollback(
                    self,
                    control_checkpoint,
                    system,
                    system_checkpoint,
                    SimulationStage::Step,
                    sample.step_index,
                    cause,
                );
            }
        }
        if let Err(cause) = system.dispatch(&frame) {
            let step_index = frame.samples.last().map_or(0, |sample| sample.step_index);
            return rollback(
                self,
                control_checkpoint,
                system,
                system_checkpoint,
                SimulationStage::Dispatch,
                step_index,
                cause,
            );
        }
        self.published = frame.clone();
        Ok(frame)
    }

    fn advance_candidate(&mut self, wall_dt: f32) -> Result<FrameTime, TimeError> {
        if self.mode == TimeMode::Seek {
            return Err(TimeError::WrongMode);
        }
        if !wall_dt.is_finite() || wall_dt < 0.0 {
            return Err(TimeError::InvalidWallDelta(wall_dt));
        }
        let steps = match self.mode {
            TimeMode::FixedStep => 1,
            TimeMode::WallClock => {
                let total = self.accumulator + wall_dt;
                let available = ((total / self.fixed_dt) + 1e-6).floor() as u32;
                self.accumulator = total - (available as f32 * self.fixed_dt);
                available.min(self.max_substeps.min(4))
            }
            TimeMode::Seek => unreachable!(),
        };
        let mut samples = Vec::with_capacity(steps as usize);
        for _ in 0..steps {
            self.frame_index += 1;
            self.simulation_time = self.frame_index as f32 * self.fixed_dt;
            samples.push(TimeSample {
                time: self.simulation_time,
                dt: self.fixed_dt,
                step_index: self.frame_index,
            });
        }
        Ok(frame(samples))
    }

    fn seek_candidate(&mut self, target: f32) -> Result<FrameTime, TimeError> {
        if self.mode != TimeMode::Seek {
            return Err(TimeError::WrongMode);
        }
        let (step_index, normalized) = self.normalize_seek(target)?;
        let mut samples = Vec::with_capacity(step_index as usize);
        for index in 1..=step_index {
            samples.push(TimeSample {
                time: index as f32 * self.fixed_dt,
                dt: self.fixed_dt,
                step_index: index,
            });
        }
        self.accumulator = 0.0;
        self.frame_index = step_index;
        self.simulation_time = normalized;
        Ok(frame(samples))
    }

    fn normalize_seek(&self, target: f32) -> Result<(u64, f32), TimeError> {
        if !target.is_finite() || target < 0.0 {
            return Err(TimeError::InvalidTarget { target });
        }
        let rounded = (target / self.fixed_dt).round();
        if !rounded.is_finite() || rounded < 0.0 || rounded > u64::MAX as f32 {
            return Err(TimeError::InvalidTarget { target });
        }
        let step_index = rounded as u64;
        if step_index > u64::from(self.max_seek_steps) {
            return Err(TimeError::SeekLimitExceeded {
                target,
                max_seek_steps: self.max_seek_steps,
            });
        }
        let normalized = step_index as f32 * self.fixed_dt;
        let tolerance = 1e-6 * target.max(1.0);
        if (target - normalized).abs() > tolerance {
            return Err(TimeError::NotOnStep { target });
        }
        Ok((step_index, normalized))
    }
}

fn validate_config(
    simulation_time: f32,
    fixed_dt: f32,
    max_substeps: u32,
    max_seek_steps: u32,
) -> Result<(), TimeError> {
    if !fixed_dt.is_finite() || fixed_dt <= 0.0 || fixed_dt > 0.1 {
        return Err(TimeError::InvalidConfig(
            "fixed_dt must be finite and in (0, 0.1]",
        ));
    }
    if !simulation_time.is_finite() || simulation_time < 0.0 {
        return Err(TimeError::InvalidConfig(
            "simulation_time must be finite and non-negative",
        ));
    }
    if !(1..=8).contains(&max_substeps) {
        return Err(TimeError::InvalidConfig("max_substeps must be in 1..=8"));
    }
    if !(1..=4096).contains(&max_seek_steps) {
        return Err(TimeError::InvalidConfig(
            "max_seek_steps must be in 1..=4096",
        ));
    }
    Ok(())
}

fn frame(samples: Vec<TimeSample>) -> FrameTime {
    let now = samples.last().map_or(0.0, |sample| sample.time);
    FrameTime { samples, now }
}

fn rollback<S: DeterministicSystem, T>(
    control: &mut TimeControl,
    control_checkpoint: TimeControl,
    system: &mut S,
    system_checkpoint: S::Checkpoint,
    stage: SimulationStage,
    step_index: u64,
    cause: S::Error,
) -> Result<T, TimeError> {
    *control = control_checkpoint;
    system.restore(system_checkpoint);
    Err(TimeError::Simulation(SimulationError {
        stage,
        step_index,
        cause: cause.to_string(),
    }))
}

/// Minimal checkpointable coordinator seam for later simulation runtimes.
pub trait DeterministicSystem {
    /// Complete restorable system state.
    type Checkpoint: Clone;
    /// Error returned by reset, step, or dispatch.
    type Error: Display;

    /// Captures all state that can change during a call.
    fn checkpoint(&self) -> Self::Checkpoint;
    /// Restores a previously captured state; restoration must be infallible.
    fn restore(&mut self, checkpoint: Self::Checkpoint);
    /// Resets the runtime to its deterministic initial state.
    fn reset(&mut self) -> Result<(), Self::Error>;
    /// Applies one ordered deterministic sample.
    fn step(&mut self, sample: TimeSample) -> Result<(), Self::Error>;
    /// Publishes the complete frame after all steps succeed.
    fn dispatch(&mut self, frame: &FrameTime) -> Result<(), Self::Error>;
}

/// Stage at which transactional simulation execution failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationStage {
    /// Initial-state reset.
    Reset,
    /// One deterministic simulation step.
    Step,
    /// Final frame publication.
    Dispatch,
}

/// Typed deterministic-system failure details.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("simulation {stage:?} failed at step {step_index}: {cause}")]
pub struct SimulationError {
    /// Failed operation stage.
    pub stage: SimulationStage,
    /// Step active at the failed stage.
    pub step_index: u64,
    /// Stable display form of the system error.
    pub cause: String,
}

/// Time validation or transactional execution error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum TimeError {
    /// Static configuration violates the contract.
    #[error("invalid time configuration: {0}")]
    InvalidConfig(&'static str),
    /// Seek target is non-finite, negative, or numerically unrepresentable.
    #[error("invalid seek target {target}")]
    InvalidTarget {
        /// Rejected target.
        target: f32,
    },
    /// Seek target is not aligned to a fixed step.
    #[error("seek target {target} is not on a fixed step")]
    NotOnStep {
        /// Rejected target.
        target: f32,
    },
    /// Seek target exceeds the configured replay limit.
    #[error("seek target {target} exceeds {max_seek_steps} steps")]
    SeekLimitExceeded {
        /// Rejected target.
        target: f32,
        /// Configured replay limit.
        max_seek_steps: u32,
    },
    /// Wall delta is non-finite or negative.
    #[error("invalid wall delta {0}")]
    InvalidWallDelta(f32),
    /// Operation does not apply to the configured mode.
    #[error("operation is not valid for this time mode")]
    WrongMode,
    /// Coordinator execution failed and was rolled back.
    #[error(transparent)]
    Simulation(SimulationError),
}

impl TimeError {
    /// Returns the failed simulation stage, when this is a coordinator error.
    pub fn stage(&self) -> Option<SimulationStage> {
        match self {
            Self::Simulation(error) => Some(error.stage),
            _ => None,
        }
    }
}
