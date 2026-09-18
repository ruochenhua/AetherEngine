//! Core VisualCase render, time, camera, reference, and comparison configuration.

use super::validation::*;
use super::{ReferenceState, VisualCaseError};
use crate::time::{TimeControl, TimeMode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenderConfig {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) frames: u32,
    pub(super) no_gui_overlay: bool,
    pub(super) png: String,
}

impl RenderConfig {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        if self.width == 0 || self.height == 0 || self.frames == 0 {
            return Err(invalid("render dimensions and frames must be positive"));
        }
        if !self.no_gui_overlay {
            return Err(invalid("VisualCase renders must disable the GUI overlay"));
        }
        if self.png != "rgba8" {
            return Err(invalid("render.png must be rgba8"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TimeConfig {
    pub(super) mode: TimeMode,
    pub(super) simulation_time: f32,
    pub(super) fixed_dt: f32,
    pub(super) max_substeps: u32,
    pub(super) max_seek_steps: u32,
}

impl TimeConfig {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        TimeControl::new(
            self.mode,
            self.simulation_time,
            self.fixed_dt,
            self.max_substeps,
            self.max_seek_steps,
        )
        .map(|_| ())
        .map_err(|error| invalid(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CameraConfig {
    pub(super) source: CameraSource,
    #[serde(rename = "override")]
    #[serde(deserialize_with = "required_option")]
    pub(super) camera_override: Option<CameraOverride>,
}

impl CameraConfig {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        if self.source != CameraSource::Scene {
            return Err(invalid("camera.source must be scene"));
        }
        if let Some(camera) = &self.camera_override {
            camera.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CameraSource {
    Scene,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CameraOverride {
    pub(super) position: [f32; 3],
    pub(super) rotation_xyzw: [f32; 4],
    pub(super) fov_deg: f32,
    pub(super) near: f32,
    pub(super) far: f32,
}

impl CameraOverride {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        let finite = self
            .position
            .iter()
            .chain(self.rotation_xyzw.iter())
            .all(|value| value.is_finite())
            && self.fov_deg.is_finite()
            && self.near.is_finite()
            && self.far.is_finite();
        if !finite
            || self.fov_deg <= 0.0
            || self.fov_deg >= 180.0
            || self.near <= 0.0
            || self.near >= self.far
            || self.rotation_xyzw.iter().all(|value| *value == 0.0)
        {
            return Err(invalid(
                "camera override is not finite and physically valid",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceConfig {
    pub(super) path: String,
    sha256: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) source_commit: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) os: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) driver: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) adapter: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) runner_version: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub(super) width: Option<u32>,
    #[serde(deserialize_with = "required_option")]
    pub(super) height: Option<u32>,
    #[serde(deserialize_with = "required_option")]
    pub(super) format: Option<String>,
}

impl ReferenceConfig {
    pub(super) fn pending(id: &str) -> Self {
        Self {
            path: format!("tests/reference/{id}.png"),
            sha256: None,
            source_commit: None,
            os: None,
            driver: None,
            adapter: None,
            runner_version: None,
            width: None,
            height: None,
            format: None,
        }
    }

    pub(super) fn state(&self, render: &RenderConfig) -> Result<ReferenceState, VisualCaseError> {
        validate_path(&self.path, "tests/reference", "reference")?;
        let present = [
            self.sha256.is_some(),
            self.source_commit.is_some(),
            self.os.is_some(),
            self.driver.is_some(),
            self.adapter.is_some(),
            self.runner_version.is_some(),
            self.width.is_some(),
            self.height.is_some(),
            self.format.is_some(),
        ];
        if present.iter().all(|value| !value) {
            return Ok(ReferenceState::Pending);
        }
        if !present.iter().all(|value| *value) {
            return Err(invalid(
                "reference provenance must be entirely null or complete",
            ));
        }
        let sha = self.sha256.as_deref().unwrap_or_default();
        let commit = self.source_commit.as_deref().unwrap_or_default();
        if !is_lower_hex(sha, 64) || !is_lower_hex(commit, 40) {
            return Err(invalid("reference hashes must be lowercase hexadecimal"));
        }
        for value in [&self.os, &self.driver, &self.adapter, &self.runner_version] {
            if value
                .as_deref()
                .map_or(true, |value| value.trim().is_empty())
            {
                return Err(invalid("reference provenance strings must be non-empty"));
            }
        }
        if self.width != Some(render.width)
            || self.height != Some(render.height)
            || self.format.as_deref() != Some("png-rgba8")
        {
            return Err(invalid(
                "reference dimensions or format do not match render",
            ));
        }
        Ok(ReferenceState::Releasable)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompareConfig {
    pub(super) algorithm: CompareAlgorithm,
    pub(super) exact_hash: bool,
    pub(super) allow_degraded_compare: bool,
    pub(super) ssim_min: f32,
    pub(super) mae_max: f32,
    pub(super) diff_percent_max: f32,
}

impl CompareConfig {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        let values = [self.ssim_min, self.mae_max, self.diff_percent_max];
        if values.iter().any(|value| !value.is_finite())
            || !(0.99..=1.0).contains(&self.ssim_min)
            || !(0.0..=0.01).contains(&self.mae_max)
            || !(0.0..=0.01).contains(&self.diff_percent_max)
        {
            return Err(invalid("compare thresholds exceed release bounds"));
        }
        let _ = (self.algorithm, self.exact_hash, self.allow_degraded_compare);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CompareAlgorithm {
    Rgba8Normalized,
}
