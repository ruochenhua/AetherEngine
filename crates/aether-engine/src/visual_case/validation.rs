//! Lexical and structural validation helpers for VisualCase v2.

use super::VisualCaseError;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path};

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

pub(super) fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
