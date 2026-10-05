//! Stable performance-series contracts and T9.1 aggregation.

mod aggregate;
mod draw_calls;
mod schema;

pub use aggregate::{sha256_hex, visible_entity_hash, PerformanceError};
pub use draw_calls::{capture_draw_calls, record_draw_call};
pub use schema::{
    BenchmarkCamera, BenchmarkCameraPose, BenchmarkCase, BenchmarkRender, BenchmarkTime,
    DeviceIdentity, FrameSample, MetricSummary, PerformanceProvenance, PerformanceSeries,
    SampleStatus, SeriesStatus, TimerSource, BENCHMARK_SAMPLE_COUNT, BENCHMARK_WARMUP_FRAMES,
};
