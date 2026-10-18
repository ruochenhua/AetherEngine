//! Authored resource fields retained by in-memory history and committed replay operations.

use super::{EditorHistory, HistoryEntry};
use crate::asset::CanonicalPath;
use crate::ecs::components::{MaterialAssetRef, MeshSource};
use crate::editor::operation::{EditorOperation, Inverse, MaterialAssignment, OperationLog};
use crate::editor::world_resource_usage::{material_candidate, store_candidate, MaterialBase};
use crate::editor::{ComponentRecord, WorldResourceDiagnostic, WorldResourceKind};
use crate::scene::{MaterialConfig, MeshRef};
use std::path::Path;

/// In-memory owner retaining a resource field.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HistoryResourceOwner {
    /// Successful operations available to undo.
    Undo,
    /// Undone operations available to redo.
    Redo,
    /// Operations retained in the committed in-memory replay log.
    CommittedLog,
}

/// Identifies one field within a retained operation or inverse snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryResourceLocator {
    /// Retaining history stack or committed log.
    pub owner: HistoryResourceOwner,
    /// Zero-based vector index, oldest/bottom entry first for each history stack.
    pub index: usize,
    /// Serialized log sequence, present only for committed-log records.
    pub sequence: Option<u64>,
    /// Stable operation discriminant, using the serialized snake_case spelling.
    pub operation: &'static str,
    /// Stable target name, or the captured inverse entity's name when available.
    pub entity_name: Option<String>,
    /// Semantic field/slot prefixed by `operation` or the typed `inverse` variant.
    pub field: String,
}

/// One retained authored field; distinct owners and slots are never deduplicated.
///
/// Reuses World snapshot kinds and path diagnostics. A separate locator is needed
/// because serialized operations and redo entries need not have a live ECS entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryResourceUse {
    /// Semantic resource role, independent of the path's extension.
    pub kind: WorldResourceKind,
    /// Retaining entry and exact semantic field.
    pub source: HistoryResourceLocator,
    /// Exact spelling held by the operation or inverse runtime state.
    pub authored_path: String,
    /// Lexical candidate only; does not prove existence or symlink containment.
    pub project_relative: Result<CanonicalPath, WorldResourceDiagnostic>,
}

impl EditorHistory {
    /// Observe authored paths in undo, redo, and the committed in-memory log.
    ///
    /// Ordered by owner (undo, redo, committed log), vector index, then field.
    /// Pass the loaders' absolute project root. No files are read or written,
    /// no assets loaded, and no history or World state changed. This excludes
    /// on-disk replay logs, resolved cache keys/handles and all other owners;
    /// it neither establishes scene cleanliness nor authorizes file mutations.
    pub fn resource_usage(&self, project_root: &Path) -> Vec<HistoryResourceUse> {
        let mut uses = Vec::new();
        for (owner, entries) in [
            (HistoryResourceOwner::Undo, &self.undo),
            (HistoryResourceOwner::Redo, &self.redo),
        ] {
            for (index, entry) in entries.iter().enumerate() {
                visit_entry(&mut uses, project_root, owner, index, entry);
            }
        }
        uses.extend(self.log.resource_usage(project_root));
        sort_uses(&mut uses);
        uses
    }
}

impl OperationLog {
    /// Observe authored fields of this in-memory log, ordered by index then field.
    ///
    /// Never reads `.editor.jsonl` files or resolves linked documents. Superseded
    /// inline fields of PlaceObject remain visible because the log retains them.
    pub fn resource_usage(&self, project_root: &Path) -> Vec<HistoryResourceUse> {
        let mut uses = Vec::new();
        for (index, entry) in self.entries().iter().enumerate() {
            let mut visitor = Visitor::new(
                &mut uses,
                project_root,
                HistoryResourceOwner::CommittedLog,
                index,
                Some(entry.sequence),
                &entry.operation,
            );
            visitor.operation(&entry.operation);
        }
        sort_uses(&mut uses);
        uses
    }
}

fn visit_entry(
    uses: &mut Vec<HistoryResourceUse>,
    root: &Path,
    owner: HistoryResourceOwner,
    index: usize,
    entry: &HistoryEntry,
) {
    let mut visitor = Visitor::new(uses, root, owner, index, None, &entry.operation);
    visitor.operation(&entry.operation);
    visitor.inverse(&entry.inverse);
}

fn sort_uses(uses: &mut [HistoryResourceUse]) {
    uses.sort_by(|left, right| {
        left.source
            .owner
            .cmp(&right.source.owner)
            .then_with(|| left.source.index.cmp(&right.source.index))
            .then_with(|| left.source.field.cmp(&right.source.field))
    });
}

struct Visitor<'a> {
    root: &'a Path,
    source: HistoryResourceLocator,
    uses: &'a mut Vec<HistoryResourceUse>,
}

impl<'a> Visitor<'a> {
    fn new(
        uses: &'a mut Vec<HistoryResourceUse>,
        root: &'a Path,
        owner: HistoryResourceOwner,
        index: usize,
        sequence: Option<u64>,
        operation: &EditorOperation,
    ) -> Self {
        let (operation, name) = match operation {
            EditorOperation::Rename { entity_name, .. } => ("rename", entity_name),
            EditorOperation::Copy { new_name, .. } => ("copy", new_name),
            EditorOperation::Delete { entity_name } => ("delete", entity_name),
            EditorOperation::AddComponent { entity_name, .. } => ("add_component", entity_name),
            EditorOperation::PlaceObject { object } => ("place_object", &object.name),
            EditorOperation::SetMaterial { entity_name, .. } => ("set_material", entity_name),
        };
        Self {
            root,
            source: HistoryResourceLocator {
                owner,
                index,
                sequence,
                operation,
                entity_name: Some(name.clone()),
                field: String::new(),
            },
            uses,
        }
    }

    fn push(
        &mut self,
        field: impl Into<String>,
        kind: WorldResourceKind,
        raw: &str,
        project_relative: Result<CanonicalPath, WorldResourceDiagnostic>,
    ) {
        let mut source = self.source.clone();
        source.field = field.into();
        self.uses.push(HistoryResourceUse {
            kind,
            source,
            authored_path: raw.into(),
            project_relative,
        });
    }

    fn store(&mut self, field: impl Into<String>, kind: WorldResourceKind, raw: &str) {
        self.push(field, kind, raw, store_candidate(self.root, raw));
    }

    fn operation(&mut self, operation: &EditorOperation) {
        match operation {
            EditorOperation::PlaceObject { object } => {
                if let MeshRef::File(path) = &object.mesh {
                    self.store("operation.object.mesh.File", WorldResourceKind::Mesh, path);
                }
                if let Some(path) = &object.material_asset {
                    self.store(
                        "operation.object.material_asset",
                        WorldResourceKind::MaterialAsset,
                        path,
                    );
                }
                // These are retained inline fields, not the linked document's config.
                self.material("operation.object.material.", &object.material, None);
            }
            EditorOperation::SetMaterial { assignment, .. } => match assignment {
                MaterialAssignment::Asset { path } => self.store(
                    "operation.assignment.Asset.path",
                    WorldResourceKind::MaterialAsset,
                    path,
                ),
                MaterialAssignment::Inline { material } => {
                    self.material("operation.assignment.Inline.material.", material, None)
                }
            },
            EditorOperation::AddComponent { component, .. } => {
                self.component("operation.component.", component);
            }
            EditorOperation::Rename { .. }
            | EditorOperation::Copy { .. }
            | EditorOperation::Delete { .. } => (),
        }
    }

    fn inverse(&mut self, inverse: &Inverse) {
        match inverse {
            Inverse::Restore { state } => {
                self.source.entity_name = state.name.as_ref().map(|name| name.0.clone());
                if let Some(mesh) = &state.mesh {
                    if let MeshSource::File(path) = &mesh.source {
                        self.store(
                            "inverse.Restore.mesh.source.File",
                            WorldResourceKind::Mesh,
                            path,
                        );
                    }
                }
                self.runtime_material(
                    "inverse.Restore.",
                    "material_config.",
                    state.material_config.as_ref(),
                    state.material_asset.as_ref(),
                );
            }
            Inverse::Material { entity_name, state } => {
                self.source.entity_name = Some(entity_name.clone());
                self.runtime_material(
                    "inverse.Material.",
                    "material.",
                    state.material.as_ref(),
                    state.material_asset.as_ref(),
                );
            }
            Inverse::Add { record, .. } => self.component("inverse.Add.record.", record),
            Inverse::Rename { .. } | Inverse::Delete { .. } | Inverse::Remove { .. } => (),
        }
    }

    fn component(&mut self, prefix: &str, record: &ComponentRecord) {
        match record {
            ComponentRecord::Mesh {
                source: MeshRef::File(path),
            } => {
                self.store(
                    format!("{prefix}Mesh.source.File"),
                    WorldResourceKind::Mesh,
                    path,
                );
            }
            ComponentRecord::Material { config } => {
                self.material(&format!("{prefix}Material.config."), config, None);
            }
            ComponentRecord::Mesh {
                source: MeshRef::Builtin(_),
            }
            | ComponentRecord::Transform { .. }
            | ComponentRecord::Visibility { .. }
            | ComponentRecord::Name { .. }
            | ComponentRecord::Light { .. }
            | ComponentRecord::Camera { .. }
            | ComponentRecord::Atmosphere { .. }
            | ComponentRecord::Clouds { .. } => (),
        }
    }

    fn runtime_material(
        &mut self,
        prefix: &str,
        material_field: &str,
        material: Option<&MaterialConfig>,
        linked: Option<&MaterialAssetRef>,
    ) {
        if let Some(linked) = linked {
            self.store(
                format!("{prefix}material_asset.0"),
                WorldResourceKind::MaterialAsset,
                &linked.0,
            );
        }
        if let Some(material) = material {
            self.material(&format!("{prefix}{material_field}"), material, linked);
        }
    }

    fn material(
        &mut self,
        prefix: &str,
        material: &MaterialConfig,
        linked: Option<&MaterialAssetRef>,
    ) {
        let base = linked.map_or_else(
            || Ok(MaterialBase::Inline(self.root.to_path_buf())),
            |linked| {
                store_candidate(self.root, &linked.0)
                    .map(|path| {
                        MaterialBase::Linked(
                            Path::new(path.as_str())
                                .parent()
                                .unwrap_or_else(|| Path::new(""))
                                .to_path_buf(),
                        )
                    })
                    .map_err(|_| WorldResourceDiagnostic::InvalidMaterialBase)
            },
        );
        for (field, kind, raw) in [
            (
                "albedo_texture",
                WorldResourceKind::AlbedoTexture,
                material.albedo_texture.as_deref(),
            ),
            (
                "normal_texture",
                WorldResourceKind::NormalTexture,
                material.normal_texture.as_deref(),
            ),
            (
                "orm_texture",
                WorldResourceKind::OrmTexture,
                material.orm_texture.as_deref(),
            ),
            (
                "emissive_texture",
                WorldResourceKind::EmissiveTexture,
                material.emissive_texture.as_deref(),
            ),
            (
                "transparent.texture",
                WorldResourceKind::TransparentTexture,
                material
                    .transparent
                    .as_ref()
                    .and_then(|config| config.texture.as_deref()),
            ),
        ] {
            if let Some(raw) = raw {
                self.push(
                    format!("{prefix}{field}"),
                    kind,
                    raw,
                    material_candidate(self.root, &base, raw),
                );
            }
        }
    }
}
