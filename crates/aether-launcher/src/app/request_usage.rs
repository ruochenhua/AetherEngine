//! Read-only inventory of resource paths retained by pending `App` requests.
#![allow(dead_code)] // Staged for the upcoming project-wide mutation gate inventory.

use aether_engine::{
    asset::{AssetIdError, CanonicalPath},
    editor::{ComponentRecord, EditorOperation, MaterialAssignment},
    scene::MeshRef,
};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum AppRequestOwner {
    SceneSelection,
    ProjectOpen,
    ProjectFileAction,
    AssetImport,
    ViewportAssetDrop,
    MaterialDrop,
    EditorOperation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppRequestLocator {
    pub owner: AppRequestOwner,
    pub slot: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppRequestDiagnostic {
    UnknownRequest,
    InvalidPath,
    OutsideProjectRoot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppRequestResourceKind {
    Scene,
    Mesh,
    MaterialAsset,
    Texture,
    Folder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppRequestResourceUse {
    pub source: AppRequestLocator,
    pub kind: Option<AppRequestResourceKind>,
    pub authored_path: Option<PathBuf>,
    pub project_relative: Result<CanonicalPath, AppRequestDiagnostic>,
}

/// Observe App-owned requests only. This excludes transient dialog-local paths,
/// ProjectPanel scanner state, AssetStore work/results, persisted replay/UI state,
/// and output targets such as screenshots. It does not establish global eligibility.
pub(crate) fn capture_app_request_resource_usage(app: &super::App) -> Vec<AppRequestResourceUse> {
    let mut visitor = Visitor {
        root: app.workspace.project_root(),
        rows: Vec::new(),
    };
    if let Some(index) = app.pending_load {
        if let Some(entry) = app.scene_entries.get(index) {
            visitor.push_path(
                AppRequestOwner::SceneSelection,
                "scene_entry.path",
                Some(AppRequestResourceKind::Scene),
                entry.path.clone(),
            );
        } else {
            visitor.unknown(AppRequestOwner::SceneSelection, "scene_entry.path");
        }
    }
    if let Some(path) = &app.pending_project_open {
        visitor.push_path(
            AppRequestOwner::ProjectOpen,
            "scene.path",
            Some(AppRequestResourceKind::Scene),
            path.clone(),
        );
    }
    if let Some(action) = &app.pending_project_file {
        match action {
            super::workspace::ProjectFileAction::CreateFolder { destination, .. } => visitor
                .push_path(
                    AppRequestOwner::ProjectFileAction,
                    "create_folder.destination",
                    Some(AppRequestResourceKind::Folder),
                    destination.clone(),
                ),
            super::workspace::ProjectFileAction::ImportAsset { destination } => visitor.push_path(
                AppRequestOwner::ProjectFileAction,
                "import_asset.destination",
                Some(AppRequestResourceKind::Folder),
                destination.clone(),
            ),
        }
    }
    if let Some(pending) = &app.pending_asset_import {
        visitor.push_path(
            AppRequestOwner::AssetImport,
            "source",
            None,
            pending.source.clone(),
        );
        visitor.push_path(
            AppRequestOwner::AssetImport,
            "destination",
            Some(AppRequestResourceKind::Folder),
            pending.destination.clone(),
        );
    }
    if let Some(request) = &app.pending_asset_drop {
        visitor.push_path(
            AppRequestOwner::ViewportAssetDrop,
            "resource.path",
            drop_kind(&request.relative_path),
            request.relative_path.clone(),
        );
    }
    if let Some(request) = &app.pending_material_drop {
        let (slot, kind) = match request.target {
            super::workspace::MaterialDropTarget::Asset => (
                "assignment.material_asset",
                Some(AppRequestResourceKind::MaterialAsset),
            ),
            super::workspace::MaterialDropTarget::Texture(usage) => {
                let (slot, kind) = match usage {
                    aether_engine::scene::material::TextureUsage::Albedo => {
                        ("assignment.albedo_texture", AppRequestResourceKind::Texture)
                    }
                    aether_engine::scene::material::TextureUsage::Normal => {
                        ("assignment.normal_texture", AppRequestResourceKind::Texture)
                    }
                    aether_engine::scene::material::TextureUsage::Orm => {
                        ("assignment.orm_texture", AppRequestResourceKind::Texture)
                    }
                    aether_engine::scene::material::TextureUsage::Emissive => (
                        "assignment.emissive_texture",
                        AppRequestResourceKind::Texture,
                    ),
                };
                (slot, Some(kind))
            }
        };
        visitor.push_path(
            AppRequestOwner::MaterialDrop,
            slot,
            kind,
            request.resource.relative_path.clone(),
        );
    }
    if let Some(operation) = &app.pending_editor_operation {
        visitor.operation(operation);
    }
    visitor.rows.sort_by(|left, right| {
        left.source
            .owner
            .cmp(&right.source.owner)
            .then_with(|| left.source.slot.cmp(&right.source.slot))
            .then_with(|| left.authored_path.cmp(&right.authored_path))
    });
    visitor.rows
}

fn drop_kind(path: &str) -> Option<AppRequestResourceKind> {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("obj") => {
            Some(AppRequestResourceKind::Mesh)
        }
        _ => None,
    }
}

struct Visitor<'a> {
    root: &'a Path,
    rows: Vec<AppRequestResourceUse>,
}

impl Visitor<'_> {
    fn unknown(&mut self, owner: AppRequestOwner, slot: &str) {
        self.rows.push(AppRequestResourceUse {
            source: AppRequestLocator {
                owner,
                slot: slot.into(),
            },
            kind: None,
            authored_path: None,
            project_relative: Err(AppRequestDiagnostic::UnknownRequest),
        });
    }

    fn push_path(
        &mut self,
        owner: AppRequestOwner,
        slot: &str,
        kind: Option<AppRequestResourceKind>,
        raw: impl Into<PathBuf>,
    ) {
        let raw = raw.into();
        let project_relative = resolve_path(self.root, &raw);
        self.rows.push(AppRequestResourceUse {
            source: AppRequestLocator {
                owner,
                slot: slot.into(),
            },
            kind,
            authored_path: Some(raw),
            project_relative,
        });
    }

    fn operation(&mut self, operation: &EditorOperation) {
        match operation {
            EditorOperation::PlaceObject { object } => {
                if let MeshRef::File(path) = &object.mesh {
                    self.push_path(
                        AppRequestOwner::EditorOperation,
                        "operation.object.mesh.File",
                        Some(AppRequestResourceKind::Mesh),
                        path.clone(),
                    );
                }
                if let Some(path) = &object.material_asset {
                    self.push_path(
                        AppRequestOwner::EditorOperation,
                        "operation.object.material_asset",
                        Some(AppRequestResourceKind::MaterialAsset),
                        path.clone(),
                    );
                }
                self.material("operation.object.material.", &object.material);
            }
            EditorOperation::SetMaterial { assignment, .. } => match assignment {
                MaterialAssignment::Asset { path } => self.push_path(
                    AppRequestOwner::EditorOperation,
                    "operation.assignment.Asset.path",
                    Some(AppRequestResourceKind::MaterialAsset),
                    path.clone(),
                ),
                MaterialAssignment::Inline { material } => {
                    self.material("operation.assignment.Inline.material.", material)
                }
            },
            EditorOperation::AddComponent { component, .. } => {
                self.component("operation.component.", component)
            }
            EditorOperation::Rename { .. }
            | EditorOperation::Copy { .. }
            | EditorOperation::Delete { .. } => (),
        }
    }

    fn component(&mut self, prefix: &str, component: &ComponentRecord) {
        match component {
            ComponentRecord::Mesh {
                source: MeshRef::File(path),
            } => self.push_path(
                AppRequestOwner::EditorOperation,
                &format!("{prefix}Mesh.source.File"),
                Some(AppRequestResourceKind::Mesh),
                path.clone(),
            ),
            ComponentRecord::Material { config } => {
                self.material(&format!("{prefix}Material.config."), config)
            }
            _ => (),
        }
    }

    fn material(&mut self, prefix: &str, material: &aether_engine::scene::MaterialConfig) {
        for (field, kind, path) in [
            (
                "albedo_texture",
                AppRequestResourceKind::Texture,
                material.albedo_texture.as_ref(),
            ),
            (
                "normal_texture",
                AppRequestResourceKind::Texture,
                material.normal_texture.as_ref(),
            ),
            (
                "orm_texture",
                AppRequestResourceKind::Texture,
                material.orm_texture.as_ref(),
            ),
            (
                "emissive_texture",
                AppRequestResourceKind::Texture,
                material.emissive_texture.as_ref(),
            ),
            (
                "transparent.texture",
                AppRequestResourceKind::Texture,
                material
                    .transparent
                    .as_ref()
                    .and_then(|value| value.texture.as_ref()),
            ),
        ] {
            if let Some(path) = path {
                self.push_path(
                    AppRequestOwner::EditorOperation,
                    &format!("{prefix}{field}"),
                    Some(kind),
                    path.clone(),
                );
            }
        }
    }
}

fn resolve_path(root: &Path, path: &Path) -> Result<CanonicalPath, AppRequestDiagnostic> {
    if path.to_string_lossy().contains('\0') {
        return Err(AppRequestDiagnostic::InvalidPath);
    }
    if path.is_absolute() {
        let normalized_root = normalize_absolute(root).ok_or(AppRequestDiagnostic::InvalidPath)?;
        let normalized_path = normalize_absolute(path).ok_or(AppRequestDiagnostic::InvalidPath)?;
        let Ok(relative) = normalized_path.strip_prefix(normalized_root) else {
            return Err(AppRequestDiagnostic::OutsideProjectRoot);
        };
        return CanonicalPath::new(root, relative).map_err(map_asset_id_error);
    }
    CanonicalPath::new(root, path).map_err(map_asset_id_error)
}

fn normalize_absolute(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => (),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized.is_absolute().then_some(normalized)
}

fn map_asset_id_error(error: AssetIdError) -> AppRequestDiagnostic {
    match error {
        AssetIdError::InvalidPath => AppRequestDiagnostic::InvalidPath,
        AssetIdError::PathEscape => AppRequestDiagnostic::OutsideProjectRoot,
    }
}

#[cfg(test)]
#[path = "request_usage_tests.rs"]
mod tests;
