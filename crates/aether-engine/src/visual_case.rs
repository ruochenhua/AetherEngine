//! Strict serde schema, validation, and variant materialization for VisualCase v2.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod benchmark;
mod config;
mod expectation;
mod materialize;
mod validation;

use benchmark::BenchmarkSpec;
use config::*;
pub use expectation::*;
pub use materialize::{CaseArtifacts, MaterializedCase};

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
    project_root: Option<PathBuf>,
}

impl VisualManifest {
    /// Parses and validates the schema and lexical paths without filesystem I/O.
    /// Use [`Self::from_json_in`] before consuming files from a project tree.
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
        let manifest = Self {
            cases,
            project_root: None,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates against a canonical project root, rejecting unresolved or escaped
    /// paths. Only a pending reference's final filename may be absent.
    pub fn from_json_in(input: &str, project_root: &Path) -> Result<Self, VisualCaseError> {
        let mut manifest = Self::from_json(input)?;
        let root = project_root
            .canonicalize()
            .map_err(|error| invalid(format!("cannot resolve project root: {error}")))?;
        if !root.is_dir() {
            return Err(invalid("project root must be a directory"));
        }
        for case in &manifest.cases {
            case.validate_paths(&root)?;
        }
        manifest.project_root = Some(root);
        Ok(manifest)
    }

    /// Returns validated source cases in manifest order.
    pub fn cases(&self) -> &[VisualCase] {
        &self.cases
    }

    /// Expands explicit variants into independent validated cases.
    pub fn materialize(&self) -> Result<Vec<MaterializedCase>, VisualCaseError> {
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
                    if let Some(benchmark) = variant.benchmark_override {
                        case.benchmark = Some(benchmark);
                    }
                    case.reference = ReferenceConfig::pending(&case.id);
                    case.variants = Value::Null;
                    case.validate()?;
                    if let Some(root) = &self.project_root {
                        case.validate_paths(root)?;
                    }
                    insert_unique(&mut ids, &case.id)?;
                    output.push(MaterializedCase {
                        case,
                        expected_result: Some(variant.expected_result),
                    });
                }
            } else {
                if let Some(root) = &self.project_root {
                    base.validate_paths(root)?;
                }
                insert_unique(&mut ids, &base.id)?;
                output.push(MaterializedCase {
                    case: base.clone(),
                    expected_result: None,
                });
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    benchmark: Option<BenchmarkSpec>,
    variants: Value,
}

impl VisualCase {
    fn validate_paths(&self, root: &Path) -> Result<(), VisualCaseError> {
        validate_canonical_path(root, &self.scene, "scenes", false)?;
        validate_canonical_path(
            root,
            &self.reference.path,
            "tests/reference",
            self.reference_state() == ReferenceState::Pending,
        )
    }

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
        if let Some(benchmark) = &self.benchmark {
            benchmark.validate()?;
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
                    "benchmark_override",
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    id: String,
    launcher_args_append: Vec<String>,
    #[serde(deserialize_with = "required_option")]
    time_override: Option<TimeConfig>,
    #[serde(deserialize_with = "required_option")]
    camera_override: Option<CameraConfig>,
    expected_result: ExpectedResult,
    #[serde(deserialize_with = "required_option")]
    benchmark_override: Option<BenchmarkSpec>,
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
        if let Some(benchmark) = &self.benchmark_override {
            benchmark.validate()?;
        }
        self.expected_result.validate()
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
