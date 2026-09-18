use aether_engine::time::{
    DeterministicSystem, SimulationStage, TimeControl, TimeMode, TimeSample,
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
