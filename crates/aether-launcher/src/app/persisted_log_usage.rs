//! Read-only resource inventory of the active scene's persisted operation log.
#![allow(dead_code)] // Staged for the owner-snapshot aggregator; no UI consumes this API yet.

use aether_engine::editor::{HistoryResourceUse, OperationLog};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PersistedLogState {
    NotApplicable,
    Missing,
    Complete,
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedLogResourceSnapshot {
    pub source_path: Option<PathBuf>,
    pub state: PersistedLogState,
    pub resources: Vec<HistoryResourceUse>,
}

pub(crate) fn capture_persisted_log_resource_usage(
    scene_path: Option<&Path>,
    project_root: &Path,
) -> PersistedLogResourceSnapshot {
    let Some(scene_path) = scene_path else {
        return snapshot(None, PersistedLogState::NotApplicable, Vec::new());
    };
    let source_path = super::editor::operation_log_path(scene_path);
    let root = match fs::canonicalize(project_root) {
        Ok(root) => root,
        Err(error) => return unavailable(source_path, "project root", error),
    };
    let resolved_scene = resolve_against_root(scene_path, &root);
    let canonical_scene = match fs::canonicalize(&resolved_scene) {
        Ok(scene) if scene.starts_with(&root) => scene,
        Ok(_) => {
            return unavailable_message(source_path, "active scene resolves outside project root");
        }
        Err(error) => return unavailable(source_path, "active scene", error),
    };
    let _ = canonical_scene;

    let log_target = resolve_against_root(&source_path, &root);
    let canonical_log = match fs::canonicalize(&log_target) {
        Ok(log) if log.starts_with(&root) => log,
        Ok(_) => {
            return unavailable_message(source_path, "operation log resolves outside project root");
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(&log_target) {
                Ok(_) => {
                    return unavailable_message(
                        source_path,
                        "operation log exists but its target cannot be canonicalized",
                    );
                }
                Err(metadata_error) if metadata_error.kind() != io::ErrorKind::NotFound => {
                    return unavailable(source_path, "operation log metadata", metadata_error);
                }
                Err(_) => {}
            }
            return match canonical_parent(&log_target) {
                Ok(parent) if parent.starts_with(&root) => {
                    snapshot(Some(source_path), PersistedLogState::Missing, Vec::new())
                }
                Ok(_) => {
                    unavailable_message(source_path, "operation log parent is outside project root")
                }
                Err(parent_error) => unavailable(source_path, "operation log parent", parent_error),
            };
        }
        Err(error) => return unavailable(source_path, "operation log", error),
    };

    let content = match fs::read_to_string(&canonical_log) {
        Ok(content) => content,
        Err(error) => return unavailable(source_path, "operation log", error),
    };
    match OperationLog::from_jsonl(&content) {
        Ok(log) => snapshot(
            Some(source_path),
            PersistedLogState::Complete,
            log.resource_usage(&root),
        ),
        Err(error) => {
            unavailable_message(source_path, &format!("operation log parse failed: {error}"))
        }
    }
}

fn resolve_against_root(path: &Path, root: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn canonical_parent(path: &Path) -> io::Result<PathBuf> {
    path.parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))
        .and_then(fs::canonicalize)
}

fn snapshot(
    source_path: Option<PathBuf>,
    state: PersistedLogState,
    resources: Vec<HistoryResourceUse>,
) -> PersistedLogResourceSnapshot {
    PersistedLogResourceSnapshot {
        source_path,
        state,
        resources,
    }
}

fn unavailable(
    path: PathBuf,
    label: &str,
    error: impl std::fmt::Display,
) -> PersistedLogResourceSnapshot {
    unavailable_message(path, &format!("{label} unavailable: {error}"))
}

fn unavailable_message(path: PathBuf, reason: &str) -> PersistedLogResourceSnapshot {
    snapshot(
        Some(path),
        PersistedLogState::Unavailable(reason.to_owned()),
        Vec::new(),
    )
}

#[cfg(test)]
#[path = "persisted_log_usage_tests.rs"]
mod tests;
