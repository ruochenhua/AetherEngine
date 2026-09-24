use aether_engine::particles::{
    ParticleEmitterConfig, ParticleRenderItem, ParticleRuntime, StepOutcome,
    MAX_PARTICLES_PER_EMITTER,
};
use aether_engine::renderer::transparent::TransparentBlendMode;
use aether_engine::time::{FrameTime, TimeSample};

fn config(entity_bits: u64) -> ParticleEmitterConfig {
    ParticleEmitterConfig {
        entity_bits,
        emission_rate: 0.0,
        burst: 0,
        max_particles: MAX_PARTICLES_PER_EMITTER,
        lifetime: [1.0, 1.0],
        initial_speed: [1.0, 1.0],
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        gravity: [0.0, -1.0, 0.0],
        start_size: 1.0,
        end_size: 0.0,
        start_color: [1.0, 1.0, 1.0, 1.0],
        end_color: [0.0, 0.0, 0.0, 0.0],
        blend: TransparentBlendMode::Alpha,
        seed: 7,
        paused: false,
    }
}

fn frame(step_index: u64, dt: f32) -> FrameTime {
    FrameTime {
        samples: vec![TimeSample {
            time: step_index as f32 * dt,
            dt,
            step_index,
        }],
        now: step_index as f32 * dt,
    }
}

fn published(outcome: StepOutcome) -> std::sync::Arc<aether_engine::particles::ParticleFrame> {
    match outcome {
        StepOutcome::Published(frame) | StepOutcome::Empty(frame) => frame,
        StepOutcome::ReusedPrevious { frame, .. } => frame,
    }
}

#[test]
fn same_seed_and_time_samples_produce_identical_snapshot_and_sha256() {
    let mut left = ParticleRuntime::default();
    let mut right = ParticleRuntime::default();
    let mut emitter = config(42);
    emitter.burst = 12;
    emitter.emission_rate = 30.0;
    left.configure_emitters(vec![emitter.clone()]).unwrap();
    right.configure_emitters(vec![emitter]).unwrap();

    for index in 1..=30 {
        let step = frame(index, 1.0 / 60.0);
        left.simulate(&step);
        right.simulate(&step);
    }

    assert_eq!(
        left.current_frame().state_hash,
        right.current_frame().state_hash
    );
    assert_eq!(left.current_frame().items, right.current_frame().items);
    assert_eq!(left.current_frame().state_hash.len(), 32);

    let mut changed_seed = config(42);
    changed_seed.burst = 12;
    changed_seed.emission_rate = 30.0;
    changed_seed.seed += 1;
    let mut different = ParticleRuntime::default();
    different.configure_emitters(vec![changed_seed]).unwrap();
    for index in 1..=30 {
        different.simulate(&frame(index, 1.0 / 60.0));
    }
    assert_ne!(
        left.current_frame().state_hash,
        different.current_frame().state_hash
    );
}

#[test]
fn time_control_seek_resets_and_replays_the_particle_runtime_deterministically() {
    use aether_engine::time::{TimeControl, TimeMode};

    let mut first = ParticleRuntime::default();
    let mut second = ParticleRuntime::default();
    let mut emitter = config(77);
    emitter.burst = 2;
    emitter.emission_rate = 24.0;
    first.configure_emitters(vec![emitter.clone()]).unwrap();
    second.configure_emitters(vec![emitter]).unwrap();
    let mut time_a = TimeControl::new(TimeMode::Seek, 0.0, 1.0 / 60.0, 4, 4096).unwrap();
    let mut time_b = time_a.clone();

    time_a.seek_and_step(0.5, &mut first).unwrap();
    time_b.seek_and_step(0.5, &mut second).unwrap();

    assert_eq!(first.current_frame().scene_generation, 1);
    assert_eq!(
        first.current_frame().state_hash,
        second.current_frame().state_hash
    );
    assert_eq!(first.current_frame().items, second.current_frame().items);
}

#[test]
fn burst_rate_and_lifetime_use_seconds_and_fixed_samples() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(1);
    emitter.burst = 2;
    emitter.emission_rate = 30.0;
    emitter.lifetime = [0.05, 0.05];
    runtime.configure_emitters(vec![emitter]).unwrap();

    let first = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(first.items.len(), 2);
    let second = published(runtime.simulate(&frame(2, 1.0 / 60.0)));
    assert_eq!(second.items.len(), 3);
    let fourth = published(runtime.simulate(&FrameTime {
        samples: vec![
            frame(3, 1.0 / 60.0).samples[0],
            frame(4, 1.0 / 60.0).samples[0],
        ],
        now: 4.0 * (1.0 / 60.0),
    }));
    assert!(fourth.items.iter().all(|item| item.particle_id >= 2));
}

#[test]
fn rate_accumulator_emits_after_two_half_rate_steps() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(3);
    emitter.emission_rate = 30.0;
    runtime.configure_emitters(vec![emitter]).unwrap();

    assert!(published(runtime.simulate(&frame(1, 1.0 / 60.0)))
        .items
        .is_empty());
    assert_eq!(
        published(runtime.simulate(&frame(2, 1.0 / 60.0)))
            .items
            .len(),
        1
    );
}

#[test]
fn runtime_rejects_time_samples_that_are_not_fixed_sixtieths() {
    let mut runtime = ParticleRuntime::default();
    runtime.configure_emitters(vec![config(32)]).unwrap();
    let previous = runtime.current_frame().clone();

    let outcome = runtime.simulate(&frame(1, 1.0 / 30.0));
    let StepOutcome::ReusedPrevious { frame, .. } = outcome else {
        panic!("non-60Hz samples must be rejected");
    };
    assert_eq!(frame.frame_id, previous.frame_id);
    assert_eq!(frame.state_hash, previous.state_hash);
}

#[test]
fn runtime_rejects_sample_time_that_disagrees_with_its_step_index() {
    let mut runtime = ParticleRuntime::default();
    runtime.configure_emitters(vec![config(34)]).unwrap();
    let output = runtime.simulate(&FrameTime {
        samples: vec![TimeSample {
            time: 0.25,
            dt: 1.0 / 60.0,
            step_index: 1,
        }],
        now: 0.25,
    });

    assert!(matches!(output, StepOutcome::ReusedPrevious { .. }));
    assert_eq!(runtime.current_frame().frame_id, 0);
}

#[test]
fn particle_motion_and_appearance_interpolate_from_fixed_step_state() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(33);
    emitter.burst = 1;
    emitter.lifetime = [1.0, 1.0];
    emitter.initial_speed = [0.0, 0.0];
    emitter.gravity = [0.0; 3];
    emitter.start_size = 2.0;
    emitter.end_size = 0.0;
    emitter.start_color = [1.0, 0.0, 0.0, 1.0];
    emitter.end_color = [0.0, 0.0, 1.0, 0.0];
    runtime.configure_emitters(vec![emitter]).unwrap();
    let samples = (1..=31)
        .map(|step_index| TimeSample {
            time: step_index as f32 * (1.0 / 60.0),
            dt: 1.0 / 60.0,
            step_index,
        })
        .collect();

    let output = published(runtime.simulate(&FrameTime {
        samples,
        now: 31.0 / 60.0,
    }));
    let particle = &output.items[0];
    assert!((particle.size - 1.0).abs() < 1e-5);
    assert!((particle.color[0] - 0.5).abs() < 1e-5);
    assert!((particle.color[2] - 0.5).abs() < 1e-5);
    assert!((particle.color[3] - 0.5).abs() < 1e-5);
    assert_eq!(particle.position, [0.0; 3]);
}

#[test]
fn pause_freezes_emission_and_particle_age() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(4);
    emitter.burst = 1;
    runtime.configure_emitters(vec![emitter.clone()]).unwrap();
    let before = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    emitter.paused = true;
    runtime.configure_emitters(vec![emitter]).unwrap();

    let after = published(runtime.simulate(&frame(2, 1.0 / 60.0)));
    assert_eq!(after.items, before.items);
}

#[test]
fn restart_increments_generation_clears_particles_and_restarts_ids() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(5);
    emitter.burst = 2;
    runtime.configure_emitters(vec![emitter]).unwrap();
    let before = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(before.items.len(), 2);

    runtime.restart_emitter(5).unwrap();
    assert!(runtime.current_frame().items.is_empty());
    assert_eq!(
        runtime.current_frame().scene_generation,
        before.scene_generation + 1
    );
    let restarted = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(restarted.items[0].particle_id, 0);
}

#[test]
fn removing_an_emitter_invalidates_the_published_generation() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(6);
    emitter.burst = 1;
    runtime.configure_emitters(vec![emitter]).unwrap();
    let before = published(runtime.simulate(&frame(1, 1.0 / 60.0)));

    runtime.configure_emitters(Vec::new()).unwrap();

    assert!(runtime.current_frame().items.is_empty());
    assert_eq!(
        runtime.current_frame().scene_generation,
        before.scene_generation + 1
    );
    assert_eq!(runtime.emitter_count(), 0);
}

#[test]
fn per_emitter_overflow_keeps_oldest_ids_and_reports_dropped_count() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(10);
    emitter.burst = MAX_PARTICLES_PER_EMITTER + 3;
    runtime.configure_emitters(vec![emitter]).unwrap();

    let output = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(output.items.len(), MAX_PARTICLES_PER_EMITTER as usize);
    assert_eq!(output.items.first().unwrap().particle_id, 0);
    assert_eq!(
        output.items.last().unwrap().particle_id,
        u64::from(MAX_PARTICLES_PER_EMITTER - 1)
    );
    assert_eq!(output.dropped_count, 3);
}

#[test]
fn configured_emitter_capacity_can_be_lower_than_the_global_per_emitter_cap() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(11);
    emitter.max_particles = 3;
    emitter.burst = 5;
    runtime.configure_emitters(vec![emitter]).unwrap();

    let output = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(output.items.len(), 3);
    assert_eq!(output.dropped_count, 2);
}

#[test]
fn global_budget_is_allocated_by_sorted_emitter_id() {
    let mut runtime = ParticleRuntime::default();
    let emitters = (0..5)
        .rev()
        .map(|id| {
            let mut emitter = config(id);
            emitter.burst = MAX_PARTICLES_PER_EMITTER;
            emitter
        })
        .collect();
    runtime.configure_emitters(emitters).unwrap();

    let output = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert_eq!(output.items.len(), 16_384);
    assert!(output
        .items
        .iter()
        .all(|item: &ParticleRenderItem| item.emitter_entity < 4));
    assert_eq!(output.dropped_count, 4_096);
}

#[test]
fn invalid_step_reuses_previous_frame_without_partial_publish() {
    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(20);
    emitter.burst = 1;
    runtime.configure_emitters(vec![emitter]).unwrap();
    let before = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    let failed = FrameTime {
        samples: vec![
            TimeSample {
                time: 2.0 / 60.0,
                dt: 1.0 / 60.0,
                step_index: 2,
            },
            TimeSample {
                time: f32::NAN,
                dt: 1.0 / 60.0,
                step_index: 3,
            },
        ],
        now: f32::NAN,
    };

    let outcome = runtime.simulate(&failed);
    let StepOutcome::ReusedPrevious { frame, .. } = outcome else {
        panic!("invalid sample must preserve the last published frame");
    };
    assert_eq!(frame.state_hash, before.state_hash);
    assert_eq!(runtime.current_frame().frame_id, before.frame_id);
}

#[test]
fn time_control_failure_rolls_back_runtime_generation_and_publication() {
    use aether_engine::time::{TimeControl, TimeMode};

    let mut runtime = ParticleRuntime::default();
    let mut emitter = config(21);
    emitter.emission_rate = f32::MAX;
    runtime.configure_emitters(vec![emitter]).unwrap();
    let previous = runtime.current_frame().clone();
    let mut time = TimeControl::new(TimeMode::Seek, 0.0, 1.0 / 60.0, 4, 4096).unwrap();

    assert!(time.seek_and_step(1.0 / 60.0, &mut runtime).is_err());
    assert_eq!(runtime.current_frame().frame_id, previous.frame_id);
    assert_eq!(
        runtime.current_frame().scene_generation,
        previous.scene_generation
    );
    assert_eq!(runtime.current_frame().state_hash, previous.state_hash);
    assert_eq!(time.simulation_time, 0.0);
}

#[test]
fn invalid_configuration_is_rejected_without_replacing_existing_emitters() {
    let mut runtime = ParticleRuntime::default();
    runtime.configure_emitters(vec![config(30)]).unwrap();
    let mut invalid = config(31);
    invalid.start_size = f32::NAN;

    assert!(runtime.configure_emitters(vec![invalid]).is_err());
    let frame = published(runtime.simulate(&frame(1, 1.0 / 60.0)));
    assert!(frame.items.is_empty());
    assert_eq!(runtime.emitter_count(), 1);
}
