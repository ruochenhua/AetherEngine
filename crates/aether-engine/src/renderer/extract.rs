//! Extract phase: ECS World → GPU-ready render data.
//!
//! Runs every frame before the render scheduler. Queries the ECS World for
//! renderable entities and produces `Vec<RenderBatch>` that the deferred passes
//! consume, plus `OptionalPassData` for passes that depend on optional scene
//! components such as terrain, water, atmosphere, clouds, and god rays.
//!
//! This module decouples ECS access from GPU command encoding: after extraction,
//! no render pass needs a `&World` reference.

mod optional;
pub use optional::{extract_optional_pass_data, OptionalPassData};

use crate::asset::mesh::{GpuMesh, InstanceData};
use crate::asset::texture::CpuTexture;
use crate::asset::Handle;
use crate::ecs::components::{MeshHandle, Transform, Visibility};
use crate::ecs::World;
use crate::math::{CullingVisibility, Frustum, Mat4};
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::skinned::SkinnedRenderData;
use crate::renderer::transparent::{TransparentBlendMode, TransparentMaterial};
use std::collections::HashMap;
use std::sync::Arc;

/// Grouping key for instanced batches.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    mesh: *const GpuMesh,
    material: MaterialBits,
    transparent: Option<TransparentBits>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TransparentBits {
    blend: u8,
    alpha_cutoff: Option<u32>,
}

impl From<&TransparentMaterial> for TransparentBits {
    fn from(material: &TransparentMaterial) -> Self {
        Self {
            blend: match material.blend {
                TransparentBlendMode::Alpha => 0,
                TransparentBlendMode::Additive => 1,
            },
            alpha_cutoff: material.alpha_cutoff.map(f32::to_bits),
        }
    }
}

/// Bit-level representation of `MaterialUniform` so it can be hashed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MaterialBits {
    albedo: [u32; 4],
    roughness: u32,
    metallic: u32,
    unlit: u32,
    normal_scale: u32,
    occlusion_strength: u32,
    emissive: [u32; 3],
    emissive_intensity: u32,
    albedo_texture_id: u64,
    normal_texture_id: u64,
    orm_texture_id: u64,
    emissive_texture_id: u64,
}

impl From<MaterialUniform> for MaterialBits {
    fn from(m: MaterialUniform) -> Self {
        Self {
            albedo: [
                m.albedo[0].to_bits(),
                m.albedo[1].to_bits(),
                m.albedo[2].to_bits(),
                m.albedo[3].to_bits(),
            ],
            roughness: m.roughness.to_bits(),
            metallic: m.metallic.to_bits(),
            unlit: m.unlit,
            normal_scale: m.normal_scale.to_bits(),
            occlusion_strength: m.occlusion_strength.to_bits(),
            emissive: m.emissive.map(f32::to_bits),
            emissive_intensity: m.emissive_intensity.to_bits(),
            albedo_texture_id: m.albedo_texture_id,
            normal_texture_id: m.normal_texture_id,
            orm_texture_id: m.orm_texture_id,
            emissive_texture_id: m.emissive_texture_id,
        }
    }
}

/// A group of instances sharing the same mesh and material.
#[derive(Clone)]
pub struct RenderBatch {
    /// GPU mesh shared by all instances in this batch.
    pub mesh: Arc<GpuMesh>,
    /// Material parameters shared by all instances.
    pub material: MaterialUniform,
    /// Optional albedo texture handle shared by all instances.
    pub albedo_texture: Option<Handle<CpuTexture>>,
    /// Optional normal texture handle shared by all instances.
    pub normal_texture: Option<Handle<CpuTexture>>,
    /// Optional ORM texture handle shared by all instances.
    pub orm_texture: Option<Handle<CpuTexture>>,
    /// Optional emissive texture handle shared by all instances.
    pub emissive_texture: Option<Handle<CpuTexture>>,
    /// Optional general transparent material contract.
    pub transparent_material: Option<TransparentMaterial>,
    /// Optional skinned geometry and palette for GBuffer/Shadow/WaterReflection.
    pub skinned: Option<Arc<SkinnedRenderData>>,
    /// Instances to draw.
    pub instances: Vec<InstanceData>,
}

/// Keep only opaque batches for deferred geometry, shadows, and planar reflection.
pub fn opaque_batches(batches: &Arc<[RenderBatch]>) -> Arc<[RenderBatch]> {
    Arc::from(
        batches
            .iter()
            .filter(|batch| batch.transparent_material.is_none())
            .cloned()
            .collect::<Vec<_>>(),
    )
}

/// Extract render batches from the ECS World.
///
/// Queries all entities with `(Transform, MeshHandle, MaterialUniform, Visibility)`
/// and groups opaque entities by `(mesh, material)` into instanced `RenderBatch`es.
pub fn extract_render_batches(world: &World) -> Vec<RenderBatch> {
    extract_render_batches_with_frustum_culling(world, None)
}

/// Extract render batches with optional frustum culling.
///
/// When `frustum` is `Some`, each entity's world-space AABB (computed from
/// `mesh.aabb` transformed by the entity's model matrix) is tested against the
/// frustum. Entities fully outside are skipped. When `frustum` is `None`, all
/// visible entities are included, matching [`extract_render_batches`].
pub fn extract_render_batches_with_frustum_culling(
    world: &World,
    frustum: Option<&Frustum>,
) -> Vec<RenderBatch> {
    extract_render_batches_with_instancing(world, frustum, true)
}

/// Extract with optional opaque instancing; transparent items stay ordered individually.
pub fn extract_render_batches_with_instancing(
    world: &World,
    frustum: Option<&Frustum>,
    instancing_enabled: bool,
) -> Vec<RenderBatch> {
    let mut batches: HashMap<BatchKey, RenderBatch> = HashMap::with_capacity(world.len() as usize);
    let mut individual_batches = Vec::new();

    for (entity, transform, mesh_handle, material, visibility, transparent) in world
        .query::<(
            hecs::Entity,
            &Transform,
            &MeshHandle,
            &MaterialUniform,
            &Visibility,
            Option<&TransparentMaterial>,
        )>()
        .iter()
    {
        if !visibility.0 {
            continue;
        }

        let model = Mat4::from_scale_rotation_translation(
            transform.scale,
            transform.rotation,
            transform.translation,
        );

        if let Some(frustum) = frustum {
            let world_aabb = mesh_handle.mesh.aabb.transform(model);
            if world_aabb.intersects_frustum(frustum) == CullingVisibility::Invisible {
                continue;
            }
        }

        let instance = InstanceData {
            model_matrix: model.to_cols_array_2d(),
            entity_id: entity.to_bits().get() as u32,
            _pad: [0; 3],
        };

        let key = BatchKey {
            mesh: Arc::as_ptr(&mesh_handle.mesh),
            material: MaterialBits::from(*material),
            transparent: transparent.map(TransparentBits::from),
        };
        let albedo_texture = texture_handle(material.albedo_texture_id);
        let normal_texture = texture_handle(material.normal_texture_id);
        let orm_texture = texture_handle(material.orm_texture_id);
        let emissive_texture = texture_handle(material.emissive_texture_id);
        if instancing_enabled && transparent.is_none() {
            batches
                .entry(key)
                .or_insert_with(|| RenderBatch {
                    mesh: mesh_handle.mesh.clone(),
                    material: *material,
                    albedo_texture,
                    normal_texture,
                    orm_texture,
                    emissive_texture,
                    transparent_material: None,
                    skinned: None,
                    instances: Vec::new(),
                })
                .instances
                .push(instance);
        } else {
            individual_batches.push(RenderBatch {
                mesh: mesh_handle.mesh.clone(),
                material: *material,
                albedo_texture,
                normal_texture,
                orm_texture,
                emissive_texture,
                transparent_material: transparent.cloned(),
                skinned: None,
                instances: vec![instance],
            });
        }
    }

    let mut batches = batches
        .into_values()
        .chain(individual_batches)
        .collect::<Vec<_>>();
    for batch in &mut batches {
        batch
            .instances
            .sort_unstable_by_key(|instance| instance.entity_id);
    }
    batches.sort_unstable_by_key(|batch| {
        batch
            .instances
            .first()
            .map_or(u32::MAX, |instance| instance.entity_id)
    });
    batches
}

fn texture_handle(id: u64) -> Option<Handle<CpuTexture>> {
    (id != 0).then(|| Handle::<CpuTexture>::new(id))
}

#[cfg(test)]
#[path = "extract/tests.rs"]
mod tests;
