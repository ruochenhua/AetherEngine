//! Strict serde schema, validation, and variant materialization for VisualCase v2.

use crate::time::{TimeControl, TimeMode};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

mod validation;

use validation::*;

const CASE_FIELDS: &[&str] = &[
    "manifest_version",
    "id",
    "scene",
    "launcher_args",
    "render",
    "time",
    "camera",
    "reference",
    "compare",
    "criteria",
    "benchmark",
    "variants",
];

/// A validated collection of canonical VisualCase v2 objects.
#[derive(Clone, Debug)]
pub struct VisualManifest {
    cases: Vec<VisualCase>,
}

impl VisualManifest {
    /// Parses a JSON array of strict VisualCase v2 objects and validates it.
    pub fn from_json(input: &str) -> Result<Self, VisualCaseError> {
        let raw: Value = serde_json::from_str(input)?;
        let raw_cases = raw
            .as_array()
            .ok_or_else(|| invalid("manifest must be a JSON array"))?;
        if raw_cases.is_empty() {
            return Err(invalid("manifest must contain at least one case"));
        }
        for raw_case in raw_cases {
            require_fields(raw_case, CASE_FIELDS, "case")?;
            require_nested_fields(raw_case)?;
        }
        let cases: Vec<VisualCase> = serde_json::from_value(raw)?;
        let manifest = Self { cases };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Returns validated source cases in manifest order.
    pub fn cases(&self) -> &[VisualCase] {
        &self.cases
    }

    /// Expands explicit variants into independent validated cases.
    pub fn materialize(&self) -> Result<Vec<VisualCase>, VisualCaseError> {
        let mut output = Vec::new();
        let mut ids = HashSet::new();
        for base in &self.cases {
            let variants = base.parsed_variants()?;
            if let Some(variants) = variants {
                for variant in variants {
                    let mut case = base.clone();
                    case.id = format!("{}__{}", base.id, variant.id);
                    case.launcher_args.extend(variant.launcher_args_append);
                    if let Some(time) = variant.time_override {
                        case.time = time;
                    }
                    if let Some(camera) = variant.camera_override {
                        case.camera = camera;
                    }
                    case.variants = Value::Null;
                    case.validate()?;
                    insert_unique(&mut ids, &case.id)?;
                    output.push(case);
                }
            } else {
                insert_unique(&mut ids, &base.id)?;
                output.push(base.clone());
            }
        }
        Ok(output)
    }

    fn validate(&self) -> Result<(), VisualCaseError> {
        let mut ids = HashSet::new();
        for case in &self.cases {
            case.validate()?;
            insert_unique(&mut ids, &case.id)?;
        }
        Ok(())
    }
}

/// One canonical VisualCase v2 record.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualCase {
    manifest_version: u32,
    /// Stable globally unique case identifier.
    pub id: String,
    scene: String,
    /// Argument vector passed directly to the launcher without shell parsing.
    pub launcher_args: Vec<String>,
    render: RenderConfig,
    time: TimeConfig,
    camera: CameraConfig,
    reference: ReferenceConfig,
    compare: CompareConfig,
    criteria: Vec<String>,
    benchmark: Value,
    variants: Value,
}

impl VisualCase {
    /// Returns whether reference provenance is pending or release-complete.
    pub fn reference_state(&self) -> ReferenceState {
        self.reference
            .state(&self.render)
            .unwrap_or(ReferenceState::Pending)
    }

    fn validate(&self) -> Result<(), VisualCaseError> {
        if self.manifest_version != 2 {
            return Err(invalid("manifest_version must be exactly 2"));
        }
        validate_id(&self.id, "case id")?;
        validate_path(&self.scene, "scenes", "scene")?;
        if self.launcher_args.iter().any(|arg| arg.contains('\0')) {
            return Err(invalid("launcher_args must not contain NUL"));
        }
        self.render.validate()?;
        self.time.validate()?;
        self.camera.validate()?;
        self.reference.state(&self.render)?;
        self.compare.validate()?;
        if self.criteria.is_empty() || self.criteria.iter().any(|item| item.trim().is_empty()) {
            return Err(invalid("criteria must contain non-empty entries"));
        }
        if !self.benchmark.is_null() {
            return Err(invalid(
                "benchmark must be null until its schema is defined",
            ));
        }
        self.parsed_variants()?;
        Ok(())
    }

    fn parsed_variants(&self) -> Result<Option<Vec<Variant>>, VisualCaseError> {
        if self.variants.is_null() {
            return Ok(None);
        }
        let raw = self
            .variants
            .as_array()
            .ok_or_else(|| invalid("variants must be null or an array"))?;
        if raw.is_empty() {
            return Err(invalid("variants must be null instead of an empty array"));
        }
        for variant in raw {
            require_fields(
                variant,
                &[
                    "id",
                    "launcher_args_append",
                    "time_override",
                    "camera_override",
                    "expected_result",
                ],
                "variant",
            )?;
        }
        let variants: Vec<Variant> = serde_json::from_value(self.variants.clone())?;
        let mut ids = HashSet::new();
        for variant in &variants {
            variant.validate()?;
            insert_unique(&mut ids, &variant.id)?;
        }
        Ok(Some(variants))
    }
}

/// Reference readiness derived from complete provenance, never from defaults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceState {
    /// All provenance fields are explicitly null and promotion is pending.
    Pending,
    /// Every provenance field is present and valid for release comparison.
    Releasable,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderConfig {
    width: u32,
    height: u32,
    frames: u32,
    no_gui_overlay: bool,
    png: String,
}

impl RenderConfig {
    fn validate(&self) -> Result<(), VisualCaseError> {
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeConfig {
    mode: TimeMode,
    simulation_time: f32,
    fixed_dt: f32,
    max_substeps: u32,
    max_seek_steps: u32,
}

impl TimeConfig {
    fn validate(&self) -> Result<(), VisualCaseError> {
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraConfig {
    source: CameraSource,
    #[serde(rename = "override")]
    camera_override: Option<CameraOverride>,
}

impl CameraConfig {
    fn validate(&self) -> Result<(), VisualCaseError> {
        if self.source != CameraSource::Scene {
            return Err(invalid("camera.source must be scene"));
        }
        if let Some(camera) = &self.camera_override {
            camera.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CameraSource {
    Scene,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraOverride {
    position: [f32; 3],
    rotation_xyzw: [f32; 4],
    fov_deg: f32,
    near: f32,
    far: f32,
}

impl CameraOverride {
    fn validate(&self) -> Result<(), VisualCaseError> {
        let finite = self
            .position
            .iter()
            .chain(self.rotation_xyzw.iter())
            .all(|value| value.is_finite())
            && self.fov_deg.is_finite()
            && self.near.is_finite()
            && self.far.is_finite();
        if !finite
            || !(0.0..180.0).contains(&self.fov_deg)
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceConfig {
    path: String,
    sha256: Option<String>,
    source_commit: Option<String>,
    os: Option<String>,
    driver: Option<String>,
    adapter: Option<String>,
    runner_version: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    format: Option<String>,
}

impl ReferenceConfig {
    fn state(&self, render: &RenderConfig) -> Result<ReferenceState, VisualCaseError> {
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
            if value.as_deref().is_none_or(|value| value.trim().is_empty()) {
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompareConfig {
    algorithm: CompareAlgorithm,
    exact_hash: bool,
    allow_degraded_compare: bool,
    ssim_min: f32,
    mae_max: f32,
    diff_percent_max: f32,
}

impl CompareConfig {
    fn validate(&self) -> Result<(), VisualCaseError> {
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

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CompareAlgorithm {
    Rgba8Normalized,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    id: String,
    launcher_args_append: Vec<String>,
    time_override: Option<TimeConfig>,
    camera_override: Option<CameraConfig>,
    expected_result: ExpectedResult,
}

impl Variant {
    fn validate(&self) -> Result<(), VisualCaseError> {
        validate_id(&self.id, "variant id")?;
        if self
            .launcher_args_append
            .iter()
            .any(|arg| arg.contains('\0'))
        {
            return Err(invalid("variant launcher args must not contain NUL"));
        }
        if let Some(time) = &self.time_override {
            time.validate()?;
        }
        if let Some(camera) = &self.camera_override {
            camera.validate()?;
        }
        self.expected_result.validate()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ExpectedResult {
    Render {
        diagnostics: Vec<String>,
        fallbacks: Vec<String>,
        metrics: BTreeMap<String, f64>,
    },
    ExpectedError {
        code: String,
        diagnostics: Vec<String>,
    },
}

impl ExpectedResult {
    fn validate(&self) -> Result<(), VisualCaseError> {
        match self {
            Self::Render {
                diagnostics,
                fallbacks,
                metrics,
            } => {
                if metrics.values().any(|value| !value.is_finite()) {
                    return Err(invalid("variant metrics must be finite"));
                }
                let _ = (diagnostics, fallbacks);
            }
            Self::ExpectedError { code, diagnostics } => {
                if code.trim().is_empty() {
                    return Err(invalid("expected error code must be non-empty"));
                }
                let _ = diagnostics;
            }
        }
        Ok(())
    }
}

/// Strict parse or contract-validation error.
#[derive(Debug, Error)]
pub enum VisualCaseError {
    /// JSON did not match the strict serde schema.
    #[error("invalid VisualCase JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Parsed data violated a semantic contract.
    #[error("invalid VisualCase: {0}")]
    Validation(String),
}
