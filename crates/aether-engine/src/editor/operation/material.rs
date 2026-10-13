use super::super::error::EditorError;
use crate::ecs::components::{MaterialAssetRef, MaterialAssetStatus};
use crate::ecs::{Entity, World};
use crate::renderer::extracted_material::ExtractedMaterial;
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::transparent::TransparentMaterial;
use crate::scene::MaterialConfig;
use serde::{Deserialize, Serialize};

/// Stable serialized choice of inline material state or linked asset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaterialAssignment {
    /// Inline material configuration, including texture slot paths.
    Inline {
        /// Complete material configuration.
        material: MaterialConfig,
    },
    /// Link a project-relative material document.
    Asset {
        /// Path relative to the project root.
        path: String,
    },
}

/// Host-resolved runtime data for one stable `SetMaterial` operation.
///
/// GPU and texture handles remain runtime-only and are deliberately absent
/// from the serialized operation.
#[derive(Clone)]
pub struct PreparedMaterial {
    /// Material configuration resolved by the host.
    pub material: MaterialConfig,
    /// Renderer-facing material uniform.
    pub uniform: MaterialUniform,
    /// Full renderer extraction state.
    pub extracted_material: ExtractedMaterial,
    /// Linked project asset reference, when assigning an asset.
    pub material_asset: Option<MaterialAssetRef>,
    /// Optional linked asset reload status.
    pub material_asset_status: Option<MaterialAssetStatus>,
    /// Runtime transparent pass state, when this material is transparent.
    pub transparent_material: Option<TransparentMaterial>,
}

#[derive(Clone)]
pub(crate) struct MaterialRuntimeState {
    pub(crate) material: MaterialConfig,
    pub(crate) uniform: MaterialUniform,
    pub(crate) extracted_material: ExtractedMaterial,
    pub(crate) material_asset: Option<MaterialAssetRef>,
    pub(crate) material_asset_status: Option<MaterialAssetStatus>,
    pub(crate) transparent_material: Option<TransparentMaterial>,
}

impl MaterialRuntimeState {
    pub(crate) fn capture(world: &World, entity: Entity) -> Result<Self, EditorError> {
        if !world.contains(entity) {
            return Err(EditorError::MissingEntity(entity));
        }
        Ok(Self {
            material: world
                .query_one::<&MaterialConfig>(entity)
                .get()
                .map_err(|_| missing_material_component("MaterialConfig"))?
                .clone(),
            uniform: *world
                .query_one::<&MaterialUniform>(entity)
                .get()
                .map_err(|_| missing_material_component("MaterialUniform"))?,
            extracted_material: world
                .query_one::<&ExtractedMaterial>(entity)
                .get()
                .map_err(|_| missing_material_component("ExtractedMaterial"))?
                .clone(),
            material_asset: world
                .query_one::<&MaterialAssetRef>(entity)
                .get()
                .ok()
                .cloned(),
            material_asset_status: world
                .query_one::<&MaterialAssetStatus>(entity)
                .get()
                .ok()
                .cloned(),
            transparent_material: world
                .query_one::<&TransparentMaterial>(entity)
                .get()
                .ok()
                .cloned(),
        })
    }

    pub(crate) fn from_prepared(prepared: PreparedMaterial) -> Self {
        Self {
            material: prepared.material,
            uniform: prepared.uniform,
            extracted_material: prepared.extracted_material,
            material_asset: prepared.material_asset,
            material_asset_status: prepared.material_asset_status,
            transparent_material: prepared.transparent_material,
        }
    }
}

fn missing_material_component(name: &str) -> EditorError {
    EditorError::InvalidMaterialOperation(format!("target Mesh has no {name}"))
}
