use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// T9 benchmark cases use a fixed warm-up and sample window.
pub const BENCHMARK_WARMUP_FRAMES: u32 = 30;
/// T9 benchmark cases collect this many measured frames.
pub const BENCHMARK_SAMPLE_COUNT: u32 = 120;

pub(crate) fn escape_key_field(value: &str) -> String {
    value.replace('%', "%25").replace('/', "%2F")
}

/// Immutable inputs used to reproduce a benchmark run.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCase {
    /// Stable primary-case identifier.
    pub case_id: String,
    /// Scene or fixture name.
    pub scene: String,
    /// Source of the generated scene data.
    pub fixture_source: String,
    /// Canonical camera configuration identifier.
    pub camera: BenchmarkCamera,
    /// Render dimensions and format.
    pub render: BenchmarkRender,
    /// Simulation mode and fixed time parameters.
    pub time: BenchmarkTime,
    /// Number of entities requested by the fixture.
    pub entity_count: u32,
    /// Deterministic fixture seed.
    pub seed: u64,
    /// Frames discarded before measurement.
    pub warmup: u32,
    /// Number of measured samples requested.
    pub samples: u32,
    /// Feature switches used for this run.
    pub feature_flags: BTreeMap<String, bool>,
    /// Frozen source revision used as the baseline.
    pub baseline_commit: String,
}

/// Render inputs recorded with a benchmark case.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkRender {
    /// Render target width in pixels.
    pub width: u32,
    /// Render target height in pixels.
    pub height: u32,
    /// Pixel format identifier.
    pub format: String,
}

/// Simulation inputs recorded with a benchmark case.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkTime {
    /// Simulation time mode.
    pub mode: String,
    /// Simulation time in seconds.
    pub simulation_time: f64,
    /// Fixed step in seconds.
    pub fixed_dt: f64,
}

/// Camera source and optional explicit camera pose.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCamera {
    /// Scene camera source, or another stable source identifier.
    pub source: String,
    /// Explicit camera values when the scene camera is overridden.
    #[serde(rename = "override")]
    pub camera_override: Option<BenchmarkCameraPose>,
}

/// Explicit camera pose recorded for a benchmark.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCameraPose {
    /// Camera position in world space.
    pub position: [f64; 3],
    /// Camera quaternion in x, y, z, w order.
    pub rotation_xyzw: [f64; 4],
    /// Vertical field of view in degrees.
    pub fov_deg: f64,
    /// Near clip plane.
    pub near: f64,
    /// Far clip plane.
    pub far: f64,
}

/// GPU and host identity used to qualify paired comparisons.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIdentity {
    /// Human-readable device label.
    pub device: String,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// Graphics driver version.
    pub driver: String,
    /// GPU adapter name.
    pub adapter: String,
    /// Render target dimensions in pixels.
    pub resolution: [u32; 2],
}

impl DeviceIdentity {
    /// Return the stable identity key used by paired benchmark runs.
    pub fn key(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}x{}",
            escape_key_field(&self.os),
            escape_key_field(&self.arch),
            escape_key_field(&self.driver),
            escape_key_field(&self.adapter),
            self.resolution[0],
            self.resolution[1]
        )
    }
}

/// Source used to measure a timing metric.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TimerSource {
    /// CPU duration measured with std::time::Instant.
    CpuInstant,
    /// GPU duration measured with wgpu timestamp queries.
    GpuTimestampQuery,
    /// The device or runtime cannot provide this timing metric.
    Unavailable,
}

/// Whether a frame sample is eligible for aggregation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum SampleStatus {
    /// All collected values for this frame are valid.
    Valid,
    /// The frame must not contribute values to summary metrics.
    Invalid {
        /// Reason the sample was rejected.
        reason: String,
    },
}

/// Per-frame timing and workload values.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrameSample {
    /// Zero-based index within the measured sample window.
    pub frame_index: u32,
    /// Eligibility of the frame values.
    pub status: SampleStatus,
    /// CPU extraction duration in milliseconds.
    pub cpu_extract_ms: Option<f64>,
    /// GPU pass duration in milliseconds.
    pub gpu_pass_ms: Option<f64>,
    /// Number of draw calls issued by the renderer.
    pub draw_calls: u32,
    /// Visible entity count after extraction.
    pub visible_entities: u32,
    /// Renderer memory usage in bytes, when available.
    pub memory_bytes: Option<u64>,
}

/// Nearest-rank p50 and p95 values. Missing or incomplete metrics remain null.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSummary {
    /// Median in the metric's declared unit.
    pub p50: Option<f64>,
    /// 95th percentile in the metric's declared unit.
    pub p95: Option<f64>,
}

/// Completion state of a performance series.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SeriesStatus {
    /// All requested frames and metric samples are available.
    Complete,
    /// Some frames or metrics are missing, or a frame was rejected.
    Partial,
    /// No sample window was configured.
    Empty,
    /// Samples exist but none of the metric sources produced values.
    Unavailable,
}

/// Hashes and timer sources captured alongside a benchmark.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceProvenance {
    /// GPU and host identity.
    pub device: DeviceIdentity,
    /// Revision of the source tree being measured.
    pub commit: String,
    /// Fingerprint of the source tree used to build the measured executable.
    pub source_fingerprint: String,
    /// Whether the measured source differs from its reported commit.
    pub source_tree_dirty: bool,
    /// CPU timing source.
    pub cpu_timer: TimerSource,
    /// GPU timing source.
    pub gpu_timer: TimerSource,
    /// SHA-256 of decoded RGBA8 output, when captured.
    pub rgba8_sha256: Option<String>,
    /// SHA-256 of sorted visible entity identifiers, when captured.
    pub visible_entity_sha256: Option<String>,
    /// Hash of deterministic application state, when captured.
    pub state_hash: Option<String>,
}

/// Aggregated benchmark output with raw frame evidence and provenance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceSeries {
    /// Version of this JSON schema.
    pub schema_version: u32,
    /// Stable primary-case identifier.
    pub case_id: String,
    /// Scene or fixture name.
    pub scene: String,
    /// Source of the generated scene data.
    pub fixture_source: String,
    /// Requested entity count.
    pub entity_count: u32,
    /// Deterministic fixture seed.
    pub seed: u64,
    /// Feature switches used for this run.
    pub feature_flags: BTreeMap<String, bool>,
    /// Frozen source revision used as the baseline.
    pub baseline_commit: String,
    /// Number of warm-up frames discarded.
    pub warmup_frames: u32,
    /// Measured frame samples, excluding warm-up frames.
    pub samples: Vec<FrameSample>,
    /// Median draw-call count.
    pub p50_draw_calls: Option<u32>,
    /// Draw-call reduction for paired runs, if calculated.
    pub draw_reduction: Option<f64>,
    /// Median CPU extraction duration in milliseconds.
    pub p50_cpu_ms: Option<f64>,
    /// 95th percentile CPU extraction duration in milliseconds.
    pub p95_cpu_ms: Option<f64>,
    /// Median GPU pass duration in milliseconds.
    pub p50_gpu_ms: Option<f64>,
    /// 95th percentile GPU pass duration in milliseconds.
    pub p95_gpu_ms: Option<f64>,
    /// Human-readable device label.
    pub device: String,
    /// Operating system.
    pub os: String,
    /// Graphics driver version.
    pub driver: String,
    /// GPU adapter name.
    pub adapter: String,
    /// Canonical host and GPU identity key.
    pub device_key: String,
    /// Canonical scene, camera, time, and workload identity key.
    pub pair_key: String,
    /// Render target dimensions in pixels.
    pub resolution: [u32; 2],
    /// Revision of the source tree being measured.
    pub commit: String,
    /// Fingerprint of the source tree used to build the measured executable.
    pub source_fingerprint: String,
    /// Whether the measured source differs from its reported commit.
    pub source_tree_dirty: bool,
    /// CPU timing source.
    pub cpu_timer: TimerSource,
    /// GPU timing source.
    pub gpu_timer: TimerSource,
    /// SHA-256 of decoded RGBA8 output, when captured.
    pub rgba8_sha256: Option<String>,
    /// SHA-256 of sorted visible entity identifiers, when captured.
    pub visible_entity_sha256: Option<String>,
    /// Hash of deterministic application state, when captured.
    pub state_hash: Option<String>,
    /// Overall availability of the collected series.
    pub status: SeriesStatus,
}
