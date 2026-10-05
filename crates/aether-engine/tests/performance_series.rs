use aether_engine::performance::{
    BenchmarkCamera, BenchmarkCase, BenchmarkRender, BenchmarkTime, DeviceIdentity, FrameSample,
    PerformanceProvenance, PerformanceSeries, SampleStatus, SeriesStatus, TimerSource,
    BENCHMARK_SAMPLE_COUNT, BENCHMARK_WARMUP_FRAMES,
};
use std::collections::BTreeMap;

fn case() -> BenchmarkCase {
    BenchmarkCase {
        case_id: "t9_baseline_report".into(),
        scene: "t9_baseline_report".into(),
        fixture_source: "fixture".into(),
        camera: BenchmarkCamera {
            source: "scene".into(),
            camera_override: None,
        },
        render: BenchmarkRender {
            width: 1280,
            height: 720,
            format: "rgba8".into(),
        },
        time: BenchmarkTime {
            mode: "fixed_step".into(),
            simulation_time: 0.0,
            fixed_dt: 1.0 / 60.0,
        },
        entity_count: 1000,
        seed: 9001,
        warmup: BENCHMARK_WARMUP_FRAMES,
        samples: BENCHMARK_SAMPLE_COUNT,
        feature_flags: BTreeMap::from([("instancing".into(), false), ("visibility".into(), false)]),
        baseline_commit: "0725ce0a36e25de4ea9f292208d44e4989cd4668".into(),
    }
}

fn provenance(gpu_timer: TimerSource) -> PerformanceProvenance {
    PerformanceProvenance {
        device: DeviceIdentity {
            device: "test device".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            driver: "test driver".into(),
            adapter: "test adapter".into(),
            resolution: [1280, 720],
        },
        commit: "7c6792825add554a8ded1eb16e6355ccda31904f".into(),
        source_fingerprint: "d".repeat(64),
        source_tree_dirty: true,
        cpu_timer: TimerSource::CpuInstant,
        gpu_timer,
        rgba8_sha256: Some("a".repeat(64)),
        visible_entity_sha256: Some("b".repeat(64)),
        state_hash: Some("c".repeat(64)),
    }
}

fn samples() -> Vec<FrameSample> {
    (0..BENCHMARK_SAMPLE_COUNT)
        .map(|frame_index| FrameSample {
            frame_index,
            status: SampleStatus::Valid,
            cpu_extract_ms: Some(f64::from(frame_index + 1)),
            gpu_pass_ms: Some(f64::from(frame_index + 2)),
            draw_calls: frame_index + 10,
            visible_entities: 1000,
            memory_bytes: None,
        })
        .collect()
}

#[test]
fn nearest_rank_aggregation_preserves_the_raw_sample_window() {
    let series = PerformanceSeries::from_capture(
        case(),
        provenance(TimerSource::GpuTimestampQuery),
        samples(),
    )
    .unwrap();

    assert_eq!(series.status, SeriesStatus::Complete);
    assert_eq!(series.samples.len(), 120);
    assert_eq!(series.p50_draw_calls, Some(69));
    assert_eq!(series.p50_cpu_ms, Some(60.0));
    assert_eq!(series.p95_cpu_ms, Some(114.0));
    assert_eq!(series.p50_gpu_ms, Some(61.0));
    assert_eq!(series.p95_gpu_ms, Some(115.0));
    assert_eq!(
        series.device_key,
        "windows/x86_64/test driver/test adapter/1280x720"
    );
}

#[test]
fn unavailable_gpu_timing_remains_null_without_invalidating_cpu_series() {
    let mut raw = samples();
    for sample in &mut raw {
        sample.gpu_pass_ms = None;
    }
    let series =
        PerformanceSeries::from_capture(case(), provenance(TimerSource::Unavailable), raw).unwrap();
    let json = serde_json::to_value(series).unwrap();

    assert_eq!(json["status"], "Complete");
    assert_eq!(json["p50_gpu_ms"], serde_json::Value::Null);
    assert_eq!(json["p95_gpu_ms"], serde_json::Value::Null);
    assert_eq!(json["gpu_timer"], "Unavailable");
    assert_eq!(json["p50_cpu_ms"], 60.0);
}

#[test]
fn invalid_sample_is_excluded_and_short_metric_series_stays_null() {
    let mut raw = samples();
    raw[10].status = SampleStatus::Invalid {
        reason: "device lost".into(),
    };
    let series =
        PerformanceSeries::from_capture(case(), provenance(TimerSource::GpuTimestampQuery), raw)
            .unwrap();

    assert_eq!(series.status, SeriesStatus::Partial);
    assert_eq!(series.p50_cpu_ms, None);
    assert_eq!(series.p95_gpu_ms, None);
}

#[test]
fn all_invalid_samples_are_unavailable_and_not_reported_as_zero() {
    let raw = samples()
        .into_iter()
        .map(|mut sample| {
            sample.cpu_extract_ms = None;
            sample.gpu_pass_ms = None;
            sample.memory_bytes = None;
            sample.status = SampleStatus::Invalid {
                reason: "no usable metric source".into(),
            };
            sample
        })
        .collect();
    let series =
        PerformanceSeries::from_capture(case(), provenance(TimerSource::Unavailable), raw).unwrap();

    assert_eq!(series.status, SeriesStatus::Unavailable);
    assert_eq!(series.p50_cpu_ms, None);
    assert_eq!(series.p50_draw_calls, None);
}

#[test]
fn zero_requested_samples_produce_an_empty_series() {
    let mut empty = case();
    empty.samples = 0;
    let series =
        PerformanceSeries::from_capture(empty, provenance(TimerSource::Unavailable), vec![])
            .unwrap();

    assert_eq!(series.status, SeriesStatus::Empty);
}

#[test]
fn duplicate_or_nonsequential_frame_ids_are_rejected() {
    let mut raw = samples();
    raw[1].frame_index = 3;
    assert!(PerformanceSeries::from_capture(
        case(),
        provenance(TimerSource::GpuTimestampQuery),
        raw
    )
    .is_err());
}

#[test]
fn draw_call_counter_is_scoped_to_the_render_schedule() {
    struct MockPass(u32);
    impl MockPass {
        fn draw(&mut self, _vertices: std::ops::Range<u32>, _instances: std::ops::Range<u32>) {
            self.0 += 1;
        }
    }

    let mut pass = MockPass(0);
    let (_, measured) = aether_engine::performance::capture_draw_calls(|| {
        aether_engine::counted_draw!(pass, draw, 0..3, 0..1);
        aether_engine::counted_draw!(pass, draw, 0..6, 0..1);
    });
    aether_engine::counted_draw!(pass, draw, 0..9, 0..1);

    assert_eq!(pass.0, 3);
    assert_eq!(measured, 2);
}
