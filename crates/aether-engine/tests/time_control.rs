use aether_engine::time::{
    DeterministicSystem, SimulationStage, TimeControl, TimeError, TimeMode, TimeSample,
};

fn control(mode: TimeMode, target: f32, fixed_dt: f32, max_seek_steps: u32) -> TimeControl {
    TimeControl::new(mode, target, fixed_dt, 8, max_seek_steps).unwrap()
}

#[test]
fn fixed_step_and_seek_samples_are_ordered_and_normalized() {
    let mut fixed = control(TimeMode::FixedStep, 0.0, 0.1, 16);
    assert_eq!(
        fixed.advance_frame(9.0).unwrap().samples,
        vec![sample(0.1, 1)]
    );

    let mut seek = control(TimeMode::Seek, 0.0, 0.1, 16);
    assert_eq!(seek.seek_frame(0.0).unwrap().samples, vec![]);
    assert_eq!(
        seek.seek_frame(0.5).unwrap().samples,
        vec![
            sample(0.1, 1),
            sample(0.2, 2),
            sample(0.3, 3),
            sample(0.4, 4),
            sample(0.5, 5),
        ]
    );
    let frame = seek.seek_frame(1.0).unwrap();
    assert_eq!(frame.samples.len(), 10);
    assert_eq!(frame.samples[9], sample(1.0, 10));
    assert_eq!(frame.now, 1.0);
}

#[test]
fn wall_clock_caps_catch_up_and_discards_excess_steps() {
    let mut time = control(TimeMode::WallClock, 0.0, 0.01, 8);
    let frame = time.advance_frame(0.105).unwrap();
    assert_eq!(frame.samples.len(), 4);
    assert!((time.accumulator - 0.005).abs() < 1e-6);
    assert_eq!(time.advance_frame(0.005).unwrap().samples.len(), 1);
}

#[test]
fn wall_clock_extreme_finite_deltas_leave_only_a_bounded_remainder() {
    for dt in [0.1, 0.01, f32::MIN_POSITIVE, f32::from_bits(1)] {
        for cap in [1, 3, 8] {
            let mut time = TimeControl::new(TimeMode::WallClock, 0.0, dt, cap, 8).unwrap();
            for delta in [f32::MAX, f32::MAX, 1e30] {
                assert_eq!(
                    time.advance_frame(delta).unwrap().samples.len(),
                    cap.min(4) as usize
                );
                assert!(time.accumulator.is_finite());
                assert!((0.0..dt).contains(&time.accumulator));
                let remainder = time.accumulator;
                let frame = time.advance_frame(0.0).unwrap();
                assert!(frame.samples.is_empty());
                assert_eq!(frame.now, 0.0);
                assert_eq!(time.accumulator, remainder);
            }
        }
    }
}

#[test]
fn seek_construction_normalizes_targets_at_the_tolerance_boundary() {
    let target = 0.5_f32 + 0.00000095;
    assert_ne!(target, 0.5);
    let time = control(TimeMode::Seek, target, 0.1, 16);
    assert_eq!(time.simulation_time, 0.5);
    assert_eq!(time.frame_index, 5);
    assert_eq!(time.published_frame().now, 0.0);
    assert!(matches!(
        TimeControl::new(TimeMode::Seek, 0.5 + 0.0000011, 0.1, 4, 16),
        Err(TimeError::NotOnStep { .. })
    ));
}

#[test]
fn seek_accepts_exactly_4096_steps_and_rejects_the_next_without_mutation() {
    let mut time = control(TimeMode::Seek, 0.0, 0.1, 4096);
    let frame = time.seek_frame(4096.0 * 0.1).unwrap();
    assert_eq!(frame.samples.len(), 4096);
    assert_eq!(frame.samples.last().unwrap().step_index, 4096);
    assert_eq!(frame.now, 4096.0 * 0.1);
    let before = time.clone();
    assert!(matches!(
        time.seek_frame(4097.0 * 0.1),
        Err(TimeError::SeekLimitExceeded {
            max_seek_steps: 4096,
            ..
        })
    ));
    assert_eq!(time, before);
    assert!(TimeControl::new(TimeMode::Seek, 0.0, 0.1, 4, 4097).is_err());
}

#[test]
fn invalid_or_over_limit_seek_preserves_control_and_publication() {
    let mut time = control(TimeMode::Seek, 0.0, 0.1, 5);
    time.seek_frame(0.5).unwrap();
    let before = time.clone();

    assert!(time.seek_frame(0.35).is_err());
    assert_eq!(time, before);
    assert!(time.seek_frame(1.0).is_err());
    assert_eq!(time, before);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TinySystem {
    value: u64,
    generation: u64,
    published_millis: u64,
    fail_reset: bool,
    fail_step: Option<u64>,
    fail_dispatch: bool,
}

impl TinySystem {
    fn healthy() -> Self {
        Self {
            value: 0,
            generation: 7,
            published_millis: 0,
            fail_reset: false,
            fail_step: None,
            fail_dispatch: false,
        }
    }

    fn state_hash(&self) -> u64 {
        self.value.rotate_left(13) ^ self.generation ^ self.published_millis
    }
}

impl DeterministicSystem for TinySystem {
    type Checkpoint = Self;
    type Error = &'static str;

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        *self = checkpoint;
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.value = 0;
        self.generation = 8;
        if self.fail_reset {
            return Err("injected reset failure");
        }
        Ok(())
    }

    fn step(&mut self, sample: TimeSample) -> Result<(), Self::Error> {
        self.value = self.value.wrapping_mul(31).wrapping_add(sample.step_index);
        if self.fail_step == Some(sample.step_index) {
            return Err("injected step failure");
        }
        Ok(())
    }

    fn dispatch(&mut self, frame: &aether_engine::time::FrameTime) -> Result<(), Self::Error> {
        self.published_millis = (frame.now * 1000.0).round() as u64;
        if self.fail_dispatch {
            return Err("injected dispatch failure");
        }
        Ok(())
    }
}

#[test]
fn repeated_seek_has_identical_frame_and_state_hash() {
    let mut time = control(TimeMode::Seek, 0.0, 0.1, 16);
    let mut system = TinySystem::healthy();
    let first = time.seek_and_step(0.5, &mut system).unwrap();
    let first_hash = system.state_hash();
    let second = time.seek_and_step(0.5, &mut system).unwrap();

    assert_eq!(second, first);
    assert_eq!(system.state_hash(), first_hash);
}

#[test]
fn advance_failures_and_rejected_deltas_preserve_state_and_error_details() {
    for mode in [TimeMode::FixedStep, TimeMode::WallClock] {
        let mut time = control(mode, 0.0, 0.1, 16);
        let mut system = TinySystem::healthy();
        time.advance_and_step(0.1, &mut system).unwrap();
        for delta in [f32::NAN, f32::INFINITY, -1.0] {
            let before_time = time.clone();
            let before_system = system.clone();
            assert!(matches!(
                time.advance_and_step(delta, &mut system),
                Err(TimeError::InvalidWallDelta(_))
            ));
            assert_eq!(time, before_time);
            assert_eq!(system, before_system);
        }
        for stage in [SimulationStage::Step, SimulationStage::Dispatch] {
            system.fail_step = (stage == SimulationStage::Step).then_some(2);
            system.fail_dispatch = stage == SimulationStage::Dispatch;
            let before_time = time.clone();
            let before_system = system.clone();
            let TimeError::Simulation(error) = time.advance_and_step(0.1, &mut system).unwrap_err()
            else {
                panic!("expected typed simulation error");
            };
            assert_eq!(error.stage, stage);
            assert_eq!(error.step_index, 2);
            assert_eq!(
                error.cause,
                if stage == SimulationStage::Step {
                    "injected step failure"
                } else {
                    "injected dispatch failure"
                }
            );
            assert_eq!(time, before_time);
            assert_eq!(system, before_system);
        }
    }
}

#[test]
fn rejected_seek_and_each_failure_stage_roll_back_all_state() {
    let mut time = control(TimeMode::Seek, 0.0, 0.1, 16);
    let mut system = TinySystem::healthy();
    time.seek_and_step(0.5, &mut system).unwrap();

    let baseline_time = time.clone();
    let baseline_system = system.clone();
    assert!(time.seek_and_step(0.35, &mut system).is_err());
    assert_eq!(time, baseline_time);
    assert_eq!(system, baseline_system);

    for (stage, reset, step, dispatch) in [
        (SimulationStage::Reset, true, None, false),
        (SimulationStage::Step, false, Some(2), false),
        (SimulationStage::Dispatch, false, None, true),
    ] {
        system.fail_reset = reset;
        system.fail_step = step;
        system.fail_dispatch = dispatch;
        let before_time = time.clone();
        let before_system = system.clone();
        let error = time.seek_and_step(0.5, &mut system).unwrap_err();
        assert_eq!(error.stage(), Some(stage));
        let TimeError::Simulation(details) = error else {
            panic!("expected typed simulation error")
        };
        let (index, cause) = match stage {
            SimulationStage::Reset => (0, "injected reset failure"),
            SimulationStage::Step => (2, "injected step failure"),
            SimulationStage::Dispatch => (5, "injected dispatch failure"),
        };
        assert_eq!(details.step_index, index);
        assert_eq!(details.cause, cause);
        assert_eq!(system.state_hash(), before_system.state_hash());
        assert_eq!(time, before_time);
        assert_eq!(system, before_system);
    }
}

fn sample(time: f32, step_index: u64) -> TimeSample {
    TimeSample {
        time,
        dt: 0.1,
        step_index,
    }
}
