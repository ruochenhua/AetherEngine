use aether_engine::performance::{
    visible_entity_hash, BenchmarkCase, DeviceIdentity, FrameSample, PerformanceProvenance,
    PerformanceSeries, SampleStatus, TimerSource,
};
use aether_engine::renderer::visibility::{LodLevel, VisibilityFrame};
use serde::Serialize;
use std::path::PathBuf;

pub(crate) struct BenchmarkCapture {
    case: BenchmarkCase,
    output: PathBuf,
    visibility_output: Option<PathBuf>,
    commit: String,
    source_fingerprint: String,
    source_tree_dirty: bool,
    frames_presented: u32,
    samples: Vec<FrameSample>,
    visibility_samples: Vec<VisibilitySample>,
    visible_entity_sha256: Option<String>,
}

#[derive(Serialize)]
struct VisibilitySeries {
    schema_version: u32,
    visibility_enabled: bool,
    lod_enabled: bool,
    samples: Vec<VisibilitySample>,
}

#[derive(Clone, Serialize)]
struct VisibilitySample {
    frame_index: u32,
    visible_entities: u32,
    culled_entities: u32,
    lod_high: u32,
    lod_medium: u32,
    lod_low: u32,
}

impl BenchmarkCapture {
    pub(crate) fn new(
        case: BenchmarkCase,
        output: PathBuf,
        visibility_output: Option<PathBuf>,
        commit: String,
        source_fingerprint: String,
        source_tree_dirty: bool,
    ) -> Self {
        Self {
            case,
            output,
            visibility_output,
            commit,
            source_fingerprint,
            source_tree_dirty,
            frames_presented: 0,
            samples: Vec::new(),
            visibility_samples: Vec::new(),
            visible_entity_sha256: None,
        }
    }

    pub(crate) fn visibility_enabled(&self) -> bool {
        self.case
            .feature_flags
            .get("visibility")
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn lod_enabled(&self) -> bool {
        self.case.feature_flags.get("lod").copied().unwrap_or(false)
    }

    pub(crate) fn instancing_enabled(&self) -> bool {
        self.case
            .feature_flags
            .get("instancing")
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn record_frame(
        &mut self,
        cpu_extract_ms: f64,
        draw_calls: u64,
        entity_ids: &[u32],
        visibility: Option<&VisibilityFrame>,
    ) {
        let frame_index = self.frames_presented;
        self.frames_presented = self.frames_presented.saturating_add(1);
        let mut sorted_ids = entity_ids.to_vec();
        sorted_ids.sort_unstable();
        sorted_ids.dedup();
        let visible_count = sorted_ids.len().min(u32::MAX as usize) as u32;
        self.visible_entity_sha256 = Some(visible_entity_hash(&sorted_ids));

        if frame_index < self.case.warmup || self.samples.len() >= self.case.samples as usize {
            return;
        }

        let accounted_entity_count = visibility.map_or(visible_count, |frame| {
            frame
                .visible_entities()
                .saturating_add(frame.culled_entities)
        });
        let status = if accounted_entity_count == self.case.entity_count {
            SampleStatus::Valid
        } else {
            SampleStatus::Invalid {
                reason: format!(
                    "fixture expected {} accounted entities, found {accounted_entity_count} ({visible_count} rendered)",
                    self.case.entity_count,
                ),
            }
        };
        self.samples.push(FrameSample {
            frame_index: self.samples.len() as u32,
            status,
            cpu_extract_ms: Some(cpu_extract_ms),
            gpu_pass_ms: None,
            draw_calls: draw_calls.min(u32::MAX as u64) as u32,
            visible_entities: visible_count,
            memory_bytes: None,
        });
        if let Some(visibility) = visibility {
            let mut lod_counts = [0_u32; 3];
            for item in &visibility.items {
                let index = match item.lod {
                    LodLevel::High => 0,
                    LodLevel::Medium => 1,
                    LodLevel::Low => 2,
                };
                lod_counts[index] = lod_counts[index].saturating_add(1);
            }
            self.visibility_samples.push(VisibilitySample {
                frame_index: self.samples.len() as u32 - 1,
                visible_entities: visibility.visible_entities(),
                culled_entities: visibility.culled_entities,
                lod_high: lod_counts[0],
                lod_medium: lod_counts[1],
                lod_low: lod_counts[2],
            });
        }
    }

    pub(crate) fn write_report(
        self,
        device: DeviceIdentity,
        rgba8_sha256: Option<String>,
        state_hash: Option<String>,
    ) -> anyhow::Result<PathBuf> {
        let visibility_enabled = self.visibility_enabled();
        let lod_enabled = self.lod_enabled();
        let visibility_output = self.visibility_output.clone();
        if visibility_enabled
            && (visibility_output.is_none() || self.visibility_samples.len() != self.samples.len())
        {
            anyhow::bail!("visibility-enabled benchmark is missing per-frame visibility metrics");
        }
        let provenance = PerformanceProvenance {
            device,
            commit: self.commit,
            source_fingerprint: self.source_fingerprint,
            source_tree_dirty: self.source_tree_dirty,
            cpu_timer: TimerSource::CpuInstant,
            gpu_timer: TimerSource::Unavailable,
            rgba8_sha256,
            visible_entity_sha256: self.visible_entity_sha256,
            state_hash,
        };
        let series = PerformanceSeries::from_capture(self.case, provenance, self.samples)?;
        if let Some(parent) = self.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.output, serde_json::to_vec_pretty(&series)?)?;
        if let Some(path) = &visibility_output {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let visibility = VisibilitySeries {
                schema_version: 1,
                visibility_enabled,
                lod_enabled,
                samples: self.visibility_samples,
            };
            std::fs::write(path, serde_json::to_vec_pretty(&visibility)?)?;
        }
        Ok(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{
        math::{Aabb, Vec3},
        performance::{BenchmarkCamera, BenchmarkRender, BenchmarkTime, SampleStatus},
        renderer::visibility::{BatchKey, VisibleItem},
    };
    use std::collections::BTreeMap;

    fn capture(entity_count: u32) -> BenchmarkCapture {
        let mut feature_flags = BTreeMap::new();
        feature_flags.insert("visibility".to_owned(), true);
        feature_flags.insert("lod".to_owned(), true);
        BenchmarkCapture::new(
            BenchmarkCase {
                case_id: "visibility-test".to_owned(),
                scene: "fixture".to_owned(),
                fixture_source: "test".to_owned(),
                camera: BenchmarkCamera {
                    source: "scene".to_owned(),
                    camera_override: None,
                },
                render: BenchmarkRender {
                    width: 64,
                    height: 64,
                    format: "rgba8".to_owned(),
                },
                time: BenchmarkTime {
                    mode: "fixed_step".to_owned(),
                    simulation_time: 0.0,
                    fixed_dt: 1.0 / 60.0,
                },
                entity_count,
                seed: 1,
                warmup: 0,
                samples: 1,
                feature_flags,
                baseline_commit: "test".to_owned(),
            },
            "unused.json".into(),
            Some("unused-visibility.json".into()),
            "test".to_owned(),
            "test".to_owned(),
            true,
        )
    }

    #[test]
    fn visibility_sample_is_valid_when_visible_and_culled_cover_fixture() {
        let mut capture = capture(1_000);
        let bounds = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let visibility = VisibilityFrame {
            items: (0..100)
                .map(|entity_bits| VisibleItem {
                    entity_bits,
                    batch_key: BatchKey::default(),
                    lod: LodLevel::Low,
                    bounds,
                })
                .collect(),
            culled_entities: 900,
            lod_by_entity: Default::default(),
        };
        let entity_ids: Vec<_> = (0..100).collect();
        capture.record_frame(0.1, 4, &entity_ids, Some(&visibility));
        assert_eq!(capture.samples.len(), 1);
        assert_eq!(capture.samples[0].visible_entities, 100);
        assert_eq!(capture.samples[0].status, SampleStatus::Valid);
    }

    #[test]
    fn visibility_sample_is_invalid_when_visible_and_culled_do_not_cover_fixture() {
        let mut capture = capture(1_000);
        let visibility = VisibilityFrame {
            culled_entities: 999,
            ..VisibilityFrame::default()
        };
        capture.record_frame(0.1, 0, &[], Some(&visibility));
        assert!(matches!(
            capture.samples[0].status,
            SampleStatus::Invalid { .. }
        ));
    }
}
