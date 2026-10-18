//! Observational inventory of authored resource fields retained by one live World.

use crate::asset::{prefab::ComponentPatch, AssetIdError, CanonicalPath};
use crate::ecs::components::{
    MaterialAssetRef, MeshHandle, MeshSource, Name, PrefabInstanceRoot, Terrain, Water,
};
use crate::ecs::{Entity, World};
use crate::scene::{MaterialConfig, MeshRef, TerrainSource};
use std::path::{Component, Path, PathBuf};

/// Semantic role of an authored resource field, independent of its file extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldResourceKind {
    /// External mesh source.
    Mesh,
    /// Linked material document.
    MaterialAsset,
    /// Placed prefab document.
    PrefabAsset,
    /// Albedo texture.
    AlbedoTexture,
    /// Normal texture.
    NormalTexture,
    /// Packed occlusion/roughness/metallic texture.
    OrmTexture,
    /// Emissive texture.
    EmissiveTexture,
    /// Transparent material texture.
    TransparentTexture,
    /// Authored terrain heightmap (currently not sampled by the terrain loader).
    TerrainHeightmap,
    /// Terrain splatmap.
    TerrainSplatmap,
    /// Terrain layer albedo.
    TerrainAlbedo,
    /// Terrain layer normal.
    TerrainNormal,
    /// Terrain layer packed roughness/metallic texture.
    TerrainRoughnessMetallic,
    /// Water distortion map.
    WaterDudv,
    /// Water normal map.
    WaterNormal,
}

/// Stable within the supplied World's entity lifetimes; names need not be unique.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldResourceLocator {
    /// Full ECS identity, including generation.
    pub entity: Entity,
    /// Authored name, when the entity has one.
    pub entity_name: Option<String>,
    /// Component retaining the field.
    pub component: &'static str,
    /// Field/slot, including layer or override index and target node where relevant.
    pub field: String,
}

/// Why a retained field has no project-relative candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldResourceDiagnostic {
    /// Empty or otherwise invalid project path.
    InvalidPath,
    /// The loader's path resolution leads outside the supplied project root.
    OutsideProjectRoot,
    /// A linked material's own path prevents resolving its texture fields.
    InvalidMaterialBase,
    /// Host texture lookup and the asset identity interpret separators differently.
    AmbiguousTextureSeparators,
}

/// One authored field retained by the supplied World, including invalid fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldResourceUse {
    /// Semantic resource role.
    pub kind: WorldResourceKind,
    /// Owning entity, component and field.
    pub source: WorldResourceLocator,
    /// Exact current spelling of the component field.
    pub authored_path: String,
    /// Lexically normalized candidate; does not prove existence or symlink containment.
    pub project_relative: Result<CanonicalPath, WorldResourceDiagnostic>,
}

/// Observe only resource fields held by `world`, in entity/component/field order.
///
/// Pass the owning loaders' absolute project root. This does not inspect files, AssetStore,
/// GPU caches, histories/replay, watchers, or pending requests. It never resolves
/// assets or changes selection. An empty result does not establish mutation safety;
/// unknown lifecycle state and other owners still require separate inspection.
pub fn capture_world_resource_usage(world: &World, project_root: &Path) -> Vec<WorldResourceUse> {
    let mut uses = Vec::new();
    for (entity, name, mesh, linked, prefab, terrain, water, material) in world
        .query::<(
            Entity,
            Option<&Name>,
            Option<&MeshHandle>,
            Option<&MaterialAssetRef>,
            Option<&PrefabInstanceRoot>,
            Option<&Terrain>,
            Option<&Water>,
            Option<&MaterialConfig>,
        )>()
        .iter()
    {
        let mut visitor = Visitor {
            entity,
            name,
            project_root,
            uses: &mut uses,
        };
        if let Some(mesh) = mesh {
            if let MeshSource::File(path) = &mesh.source {
                visitor.store("MeshHandle", "source.File", WorldResourceKind::Mesh, path);
            }
        }
        if let Some(linked) = linked {
            visitor.store(
                "MaterialAssetRef",
                "0",
                WorldResourceKind::MaterialAsset,
                &linked.0,
            );
        }
        if let Some(prefab) = prefab {
            visitor.prefab(prefab);
        }
        if let Some(terrain) = terrain {
            visitor.terrain(terrain);
        }
        if let Some(water) = water {
            for (field, kind, path) in [
                (
                    "config.dudv_map",
                    WorldResourceKind::WaterDudv,
                    &water.config.dudv_map,
                ),
                (
                    "config.normal_map",
                    WorldResourceKind::WaterNormal,
                    &water.config.normal_map,
                ),
            ] {
                if let Some(path) = path {
                    visitor.store("Water", field, kind, path);
                }
            }
        }
        if let Some(material) = material {
            // SceneLoader and live linked assignments keep the document's raw config.
            // Its texture fields retain the MaterialAsset loader's document-relative base.
            let base = linked.map_or_else(
                || Ok(project_root.to_path_buf()),
                |linked| {
                    store_candidate(project_root, &linked.0)
                        .map(|path| {
                            project_root.join(
                                Path::new(path.as_str())
                                    .parent()
                                    .unwrap_or_else(|| Path::new("")),
                            )
                        })
                        .map_err(|_| WorldResourceDiagnostic::InvalidMaterialBase)
                },
            );
            visitor.material(material, "MaterialConfig", "", &base);
        }
    }
    uses.sort_by(|left, right| {
        left.source
            .entity
            .to_bits()
            .cmp(&right.source.entity.to_bits())
            .then_with(|| left.source.component.cmp(right.source.component))
            .then_with(|| left.source.field.cmp(&right.source.field))
    });
    uses
}

struct Visitor<'a> {
    entity: Entity,
    name: Option<&'a Name>,
    project_root: &'a Path,
    uses: &'a mut Vec<WorldResourceUse>,
}

impl Visitor<'_> {
    fn push(
        &mut self,
        component: &'static str,
        field: impl Into<String>,
        kind: WorldResourceKind,
        path: &str,
        project_relative: Result<CanonicalPath, WorldResourceDiagnostic>,
    ) {
        self.uses.push(WorldResourceUse {
            kind,
            source: WorldResourceLocator {
                entity: self.entity,
                entity_name: self.name.map(|name| name.0.clone()),
                component,
                field: field.into(),
            },
            authored_path: path.into(),
            project_relative,
        });
    }

    fn store(
        &mut self,
        component: &'static str,
        field: impl Into<String>,
        kind: WorldResourceKind,
        path: &str,
    ) {
        self.push(
            component,
            field,
            kind,
            path,
            store_candidate(self.project_root, path),
        );
    }

    fn material(
        &mut self,
        material: &MaterialConfig,
        component: &'static str,
        prefix: &str,
        base: &Result<PathBuf, WorldResourceDiagnostic>,
    ) {
        for (field, kind, path) in [
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
            if let Some(path) = path {
                self.push(
                    component,
                    format!("{prefix}{field}"),
                    kind,
                    path,
                    material_candidate(self.project_root, base, path),
                );
            }
        }
    }

    fn prefab(&mut self, root: &PrefabInstanceRoot) {
        self.store(
            "PrefabInstanceRoot",
            "prefab_asset",
            WorldResourceKind::PrefabAsset,
            &root.0.prefab_asset,
        );
        for (index, patch) in root.0.overrides.patches.iter().enumerate() {
            match patch {
                ComponentPatch::MeshSource {
                    instance_id,
                    source: MeshRef::File(path),
                } => {
                    self.store(
                        "PrefabInstanceRoot",
                        format!("overrides.patches[{index}].node[{instance_id}].source"),
                        WorldResourceKind::Mesh,
                        path,
                    );
                }
                ComponentPatch::MaterialConfig {
                    instance_id,
                    config,
                } => self.material(
                    config,
                    "PrefabInstanceRoot",
                    &format!("overrides.patches[{index}].node[{instance_id}].config."),
                    &Ok(self.project_root.to_path_buf()),
                ),
                // Other typed patches contain no authored resource paths.
                ComponentPatch::MeshSource {
                    source: MeshRef::Builtin(_),
                    ..
                }
                | ComponentPatch::TransformTranslation { .. }
                | ComponentPatch::TransformRotation { .. }
                | ComponentPatch::TransformScale { .. }
                | ComponentPatch::Visibility { .. }
                | ComponentPatch::NameValue { .. }
                | ComponentPatch::LightConfig { .. }
                | ComponentPatch::CameraConfig { .. }
                | ComponentPatch::AtmosphereConfig { .. }
                | ComponentPatch::CloudsConfig { .. } => (),
            }
        }
    }

    fn terrain(&mut self, terrain: &Terrain) {
        if let TerrainSource::Heightmap(path) = &terrain.source {
            self.store(
                "Terrain",
                "source.Heightmap",
                WorldResourceKind::TerrainHeightmap,
                path,
            );
        }
        if let Some(path) = &terrain.splatmap_path {
            self.store(
                "Terrain",
                "splatmap_path",
                WorldResourceKind::TerrainSplatmap,
                path,
            );
        }
        for (index, layer) in terrain.layer_configs.iter().enumerate() {
            for (field, kind, path) in [
                (
                    "albedo_texture",
                    WorldResourceKind::TerrainAlbedo,
                    &layer.albedo_texture,
                ),
                (
                    "normal_texture",
                    WorldResourceKind::TerrainNormal,
                    &layer.normal_texture,
                ),
                (
                    "roughness_metallic_texture",
                    WorldResourceKind::TerrainRoughnessMetallic,
                    &layer.roughness_metallic_texture,
                ),
            ] {
                if let Some(path) = path {
                    self.store(
                        "Terrain",
                        format!("layer_configs[{index}].{field}"),
                        kind,
                        path,
                    );
                }
            }
        }
    }
}

// Same lexical identity policy as AssetStore::compatibility_path. Legacy external
// files can be loaded by AssetManager, but cannot be project-relative candidates.
fn store_candidate(root: &Path, raw: &str) -> Result<CanonicalPath, WorldResourceDiagnostic> {
    if raw.contains('\0') {
        return Err(WorldResourceDiagnostic::InvalidPath);
    }
    let path = Path::new(raw);
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .map_err(|_| WorldResourceDiagnostic::OutsideProjectRoot)?
    } else {
        path
    };
    CanonicalPath::new(root, relative).map_err(|error| match error {
        AssetIdError::InvalidPath => WorldResourceDiagnostic::InvalidPath,
        AssetIdError::PathEscape => WorldResourceDiagnostic::OutsideProjectRoot,
    })
}

fn material_candidate(
    root: &Path,
    base: &Result<PathBuf, WorldResourceDiagnostic>,
    raw: &str,
) -> Result<CanonicalPath, WorldResourceDiagnostic> {
    let base = base.as_ref().map_err(Clone::clone)?;
    if raw.is_empty() {
        return Err(WorldResourceDiagnostic::InvalidPath);
    }
    // MaterialResolver checks the host path before passing it to AssetManager.
    // On Unix a backslash is a literal filename character there, but a separator
    // in CanonicalPath. A single candidate would conceal this loader disagreement.
    if cfg!(unix) && raw.contains('\\') {
        return Err(WorldResourceDiagnostic::AmbiguousTextureSeparators);
    }
    let root = normalize_material_path(root);
    let resolved = normalize_material_path(&base.join(raw));
    store_candidate(
        &root,
        resolved
            .to_str()
            .ok_or(WorldResourceDiagnostic::InvalidPath)?,
    )
}

// Match scene::material::normalize_path: native host components, join first, then
// remove dot/parent components. No filesystem canonicalization or asset loading.
fn normalize_material_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
#[path = "world_resource_usage_tests.rs"]
mod tests;
