use super::schema::*;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors found while freezing a benchmark capture as a reportable series.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum PerformanceError {
    /// Case inputs do not satisfy the frozen T9.1 contract.
    #[error("invalid benchmark case: {0}")]
    InvalidCase(&'static str),
    /// Frame samples are duplicated, out of order, or exceed the requested count.
    #[error("invalid sample order or sample count")]
    InvalidSamples,
    /// A valid sample contains a non-finite or negative duration.
    #[error("valid frame sample contains an invalid duration")]
    InvalidDuration,
}

impl PerformanceSeries {
    /// Aggregate a measured window while preserving every raw frame sample.
    pub fn from_capture(
        case: BenchmarkCase,
        provenance: PerformanceProvenance,
        samples: Vec<FrameSample>,
    ) -> Result<Self, PerformanceError> {
        validate_case(&case, &provenance.device, &provenance.commit)?;
        if !is_lower_hex(&provenance.source_fingerprint, 64)
            || [
                provenance.rgba8_sha256.as_deref(),
                provenance.visible_entity_sha256.as_deref(),
                provenance.state_hash.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|hash| !is_lower_hex(hash, 64))
        {
            return Err(PerformanceError::InvalidCase(
                "source fingerprint and optional hashes must be lowercase SHA-256",
            ));
        }
        if samples.len() > case.samples as usize
            || samples
                .iter()
                .enumerate()
                .any(|(index, sample)| sample.frame_index != index as u32)
        {
            return Err(PerformanceError::InvalidSamples);
        }
        if samples.iter().any(|sample| match &sample.status {
            SampleStatus::Invalid { reason } => reason.trim().is_empty(),
            SampleStatus::Valid => false,
        }) {
            return Err(PerformanceError::InvalidSamples);
        }
        if samples.iter().any(|sample| {
            matches!(sample.status, SampleStatus::Valid)
                && [sample.cpu_extract_ms, sample.gpu_pass_ms]
                    .into_iter()
                    .flatten()
                    .any(|value| !value.is_finite() || value < 0.0)
        }) {
            return Err(PerformanceError::InvalidDuration);
        }

        let expected = case.samples as usize;
        let valid = samples
            .iter()
            .filter(|sample| sample.status == SampleStatus::Valid)
            .collect::<Vec<_>>();
        let cpu = summarize(
            valid
                .iter()
                .filter_map(|sample| sample.cpu_extract_ms)
                .collect(),
            expected,
        );
        let gpu = summarize(
            valid
                .iter()
                .filter_map(|sample| sample.gpu_pass_ms)
                .collect(),
            expected,
        );
        let draws = summarize(
            valid
                .iter()
                .map(|sample| f64::from(sample.draw_calls))
                .collect(),
            expected,
        );
        let has_metric = !valid.is_empty();
        let has_invalid = samples
            .iter()
            .any(|sample| matches!(sample.status, SampleStatus::Invalid { .. }));
        let incomplete = samples.len() < expected;
        let metric_incomplete = (provenance.cpu_timer != TimerSource::Unavailable
            && valid.iter().filter(|s| s.cpu_extract_ms.is_some()).count() < expected)
            || (provenance.gpu_timer != TimerSource::Unavailable
                && valid.iter().filter(|s| s.gpu_pass_ms.is_some()).count() < expected);
        let status = if case.samples == 0 {
            SeriesStatus::Empty
        } else if !has_metric {
            SeriesStatus::Unavailable
        } else if incomplete || has_invalid || metric_incomplete {
            SeriesStatus::Partial
        } else {
            SeriesStatus::Complete
        };
        let pair_key = format!(
            "{}/{}/{}/{}/{}/{}/{}/{}",
            escape_key_field(&case.scene),
            camera_key(&case.camera),
            case.time.mode,
            case.time.simulation_time.to_bits(),
            case.time.fixed_dt.to_bits(),
            case.seed,
            case.warmup,
            case.samples
        );
        let device = provenance.device;

        Ok(Self {
            schema_version: 1,
            case_id: case.case_id,
            scene: case.scene,
            fixture_source: case.fixture_source,
            entity_count: case.entity_count,
            seed: case.seed,
            feature_flags: case.feature_flags,
            baseline_commit: case.baseline_commit,
            warmup_frames: case.warmup,
            samples,
            p50_draw_calls: draws.p50.map(|value| value as u32),
            draw_reduction: None,
            p50_cpu_ms: cpu.p50,
            p95_cpu_ms: cpu.p95,
            p50_gpu_ms: gpu.p50,
            p95_gpu_ms: gpu.p95,
            device_key: device.key(),
            device: device.device,
            os: device.os,
            driver: device.driver,
            adapter: device.adapter,
            pair_key,
            resolution: device.resolution,
            commit: provenance.commit,
            source_fingerprint: provenance.source_fingerprint,
            source_tree_dirty: provenance.source_tree_dirty,
            cpu_timer: provenance.cpu_timer,
            gpu_timer: provenance.gpu_timer,
            rgba8_sha256: provenance.rgba8_sha256,
            visible_entity_sha256: provenance.visible_entity_sha256,
            state_hash: provenance.state_hash,
            status,
        })
    }
}

fn validate_case(
    case: &BenchmarkCase,
    device: &DeviceIdentity,
    commit: &str,
) -> Result<(), PerformanceError> {
    if case.case_id.trim().is_empty()
        || case.scene.trim().is_empty()
        || case.fixture_source.trim().is_empty()
        || case.camera.source.trim().is_empty()
        || case.render.width == 0
        || case.render.height == 0
        || case.render.format.trim().is_empty()
        || case.time.mode.trim().is_empty()
        || !case.time.simulation_time.is_finite()
        || case.time.simulation_time < 0.0
        || !case.time.fixed_dt.is_finite()
        || case.time.fixed_dt <= 0.0
        || case.entity_count == 0
        || case.warmup != BENCHMARK_WARMUP_FRAMES
        || (case.samples != 0 && case.samples != BENCHMARK_SAMPLE_COUNT)
        || !is_lower_hex(&case.baseline_commit, 40)
        || !is_lower_hex(commit, 40)
        || device.resolution.contains(&0)
        || [
            &device.device,
            &device.os,
            &device.arch,
            &device.driver,
            &device.adapter,
        ]
        .into_iter()
        .any(|value| value.trim().is_empty())
        || case.feature_flags.keys().any(|name| name.trim().is_empty())
        || case.camera.camera_override.as_ref().is_some_and(|camera| {
            camera.position.iter().any(|value| !value.is_finite())
                || camera.rotation_xyzw.iter().any(|value| !value.is_finite())
                || camera.rotation_xyzw.iter().all(|value| *value == 0.0)
                || !camera.fov_deg.is_finite()
                || !(0.0..180.0).contains(&camera.fov_deg)
                || !camera.near.is_finite()
                || !camera.far.is_finite()
                || camera.near <= 0.0
                || camera.near >= camera.far
        })
    {
        return Err(PerformanceError::InvalidCase(
            "metadata, identities, and the 30/120 window must be valid",
        ));
    }
    Ok(())
}

/// SHA-256 digest as lowercase hexadecimal.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Hash sorted, unique entity identifiers using little-endian u32 encoding.
pub fn visible_entity_hash(entity_ids: &[u32]) -> String {
    let mut sorted = entity_ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let bytes = sorted
        .iter()
        .flat_map(|entity_id| entity_id.to_le_bytes())
        .collect::<Vec<_>>();
    sha256_hex(&bytes)
}

fn summarize(mut values: Vec<f64>, expected: usize) -> MetricSummary {
    if expected == 0 || values.len() < expected {
        return MetricSummary::default();
    }
    values.sort_by(f64::total_cmp);
    let nearest_rank = |percent: usize| {
        let rank = (percent * values.len()).div_ceil(100);
        values[rank.saturating_sub(1)]
    };
    MetricSummary {
        p50: Some(nearest_rank(50)),
        p95: Some(nearest_rank(95)),
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn camera_key(camera: &BenchmarkCamera) -> String {
    let mut fields = vec![escape_key_field(&camera.source)];
    if let Some(pose) = &camera.camera_override {
        fields.extend(
            pose.position
                .iter()
                .map(|value| value.to_bits().to_string()),
        );
        fields.extend(
            pose.rotation_xyzw
                .iter()
                .map(|value| value.to_bits().to_string()),
        );
        fields.push(pose.fov_deg.to_bits().to_string());
        fields.push(pose.near.to_bits().to_string());
        fields.push(pose.far.to_bits().to_string());
    }
    fields.join(":")
}
