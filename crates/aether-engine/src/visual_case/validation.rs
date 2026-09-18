//! Structural, lexical, and project-root canonical validation for VisualCase v2.

use super::VisualCaseError;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path};

// An explicit null is allowed, but a missing nullable contract field is not.
pub(super) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(deserializer)
}

pub(super) fn nonempty(value: &str, context: &str) -> Result<(), VisualCaseError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{context} must be non-empty")));
    }
    Ok(())
}

pub(super) fn invalid(message: impl Into<String>) -> VisualCaseError {
    VisualCaseError::Validation(message.into())
}

pub(super) fn require_fields(
    value: &Value,
    fields: &[&str],
    context: &str,
) -> Result<(), VisualCaseError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(format!("{context} must be an object")))?;
    for field in fields {
        if !object.contains_key(*field) {
            return Err(invalid(format!(
                "{context} is missing required field {field}"
            )));
        }
    }
    Ok(())
}

pub(super) fn require_nested_fields(case: &Value) -> Result<(), VisualCaseError> {
    for (name, fields) in [
        (
            "render",
            &["width", "height", "frames", "no_gui_overlay", "png"][..],
        ),
        (
            "time",
            &[
                "mode",
                "simulation_time",
                "fixed_dt",
                "max_substeps",
                "max_seek_steps",
            ][..],
        ),
        ("camera", &["source", "override"][..]),
        (
            "reference",
            &[
                "path",
                "sha256",
                "source_commit",
                "os",
                "driver",
                "adapter",
                "runner_version",
                "width",
                "height",
                "format",
            ][..],
        ),
        (
            "compare",
            &[
                "algorithm",
                "exact_hash",
                "allow_degraded_compare",
                "ssim_min",
                "mae_max",
                "diff_percent_max",
            ][..],
        ),
    ] {
        require_fields(&case[name], fields, name)?;
    }
    Ok(())
}

pub(super) fn validate_id(id: &str, context: &str) -> Result<(), VisualCaseError> {
    let valid = !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if !valid {
        return Err(invalid(format!(
            "{context} must contain only ASCII letters, digits, '_' or '-'"
        )));
    }
    Ok(())
}

pub(super) fn insert_unique(ids: &mut HashSet<String>, id: &str) -> Result<(), VisualCaseError> {
    if !ids.insert(id.to_owned()) {
        return Err(invalid(format!("duplicate case or variant id {id}")));
    }
    Ok(())
}

pub(super) fn validate_path(path: &str, root: &str, context: &str) -> Result<(), VisualCaseError> {
    // Manifest paths use slash separators on every platform.
    if path.contains(['\\', '\0', ':']) {
        return Err(invalid(format!("{context} path has invalid characters")));
    }
    let path = Path::new(path);
    let safe = !path.is_absolute()
        && path.starts_with(root)
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        return Err(invalid(format!(
            "{context} path must stay inside project-relative {root}"
        )));
    }
    Ok(())
}

pub(super) fn validate_canonical_path(
    project_root: &Path,
    path: &str,
    allowed_root: &str,
    allow_missing_leaf: bool,
) -> Result<(), VisualCaseError> {
    validate_path(path, allowed_root, "asset")?;
    let resolve_error = |error| invalid(format!("cannot prove containment of {path}: {error}"));
    let allowed = project_root
        .join(allowed_root)
        .canonicalize()
        .map_err(resolve_error)?;
    if !allowed.is_dir() || !allowed.starts_with(project_root) {
        return Err(invalid(format!(
            "allowed root {allowed_root} escapes project or is not a directory"
        )));
    }
    let input = project_root.join(path);
    // symlink_metadata distinguishes an absent leaf from a dangling symlink:
    // dangling links and cycles must fail canonicalization, never become pending.
    let resolved = match std::fs::symlink_metadata(&input) {
        Ok(_) => {
            let resolved = input.canonicalize().map_err(resolve_error)?;
            if !resolved.is_file() {
                return Err(invalid(format!("{path} must be a file")));
            }
            resolved
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_missing_leaf => {
            let parent = input
                .parent()
                .ok_or_else(|| invalid("missing path parent"))?;
            let parent = parent.canonicalize().map_err(resolve_error)?;
            if !parent.is_dir() {
                return Err(invalid("pending reference parent must be a directory"));
            }
            let name = input
                .file_name()
                .ok_or_else(|| invalid("missing reference filename"))?;
            parent.join(name)
        }
        Err(error) => return Err(resolve_error(error)),
    };
    if !resolved.starts_with(&allowed) || !resolved.starts_with(project_root) {
        return Err(invalid(format!("{path} resolves outside {allowed_root}")));
    }
    Ok(())
}

pub(super) fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
