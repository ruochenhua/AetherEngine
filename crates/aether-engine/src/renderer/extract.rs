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
/// and groups them by `(mesh, material)` into `RenderBatch`es for instanced drawing.
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
    let mut batches: HashMap<BatchKey, RenderBatch> = HashMap::with_capacity(world.len() as usize);

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
        batches
            .entry(key)
            .or_insert_with(|| RenderBatch {
                mesh: mesh_handle.mesh.clone(),
                material: *material,
                albedo_texture,
                normal_texture,
                orm_texture,
                emissive_texture,
                transparent_material: transparent.cloned(),
                instances: Vec::new(),
            })
            .instances
            .push(instance);
    }

    batches.into_values().collect()
}

fn texture_handle(id: u64) -> Option<Handle<CpuTexture>> {
    (id != 0).then(|| Handle::<CpuTexture>::new(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::mesh::GpuMesh;
    use crate::asset::registry::BuiltinMeshRegistry;
    use crate::asset::terrain_material::TerrainMaterial;
    use crate::ecs::components::{Atmosphere, Clouds, GodRay, Terrain, Water};
    use crate::ecs::components::{Name, Visibility};
    use crate::ecs::World;
    use crate::math::{Frustum, Vec3};
    use crate::renderer::renderable::MaterialUniform;
    use crate::scene::{
        AtmosphereConfig, CloudConfig, GodRayConfig, TerrainGeometry, TerrainSource, WaterConfig,
    };

    fn default_terrain() -> Terrain {
        Terrain {
            source: TerrainSource::Procedural {
                seed: 0,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: TerrainGeometry::default(),
            material: TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: Vec::new(),
        }
    }
    use std::sync::Arc;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    #[test]
    fn extract_without_culling_includes_all_visible() {
        let device = headless_device();
        let registry = BuiltinMeshRegistry::new();
        let cube_cpu = registry.get("cube").unwrap();
        let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

        let mut world = World::new();
        world.spawn((
            Transform::default(),
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform::default(),
            Visibility::default(),
            Name("visible".into()),
        ));

        let batches = extract_render_batches(&world);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 1);
    }

    #[test]
    fn extract_with_culling_skips_outside_entities() {
        let device = headless_device();
        let registry = BuiltinMeshRegistry::new();
        let cube_cpu = registry.get("cube").unwrap();
        let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

        let mut world = World::new();
        // Inside the identity NDC cube.
        world.spawn((
            Transform::default(),
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform::default(),
            Visibility::default(),
            Name("inside".into()),
        ));
        // Far outside the identity NDC cube.
        world.spawn((
            Transform {
                translation: Vec3::new(10.0, 0.0, 0.0),
                ..Default::default()
            },
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform::default(),
            Visibility::default(),
            Name("outside".into()),
        ));

        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let batches = extract_render_batches_with_frustum_culling(&world, Some(&frustum));
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 1);
    }

    #[test]
    fn extract_respects_visibility_component() {
        let device = headless_device();
        let registry = BuiltinMeshRegistry::new();
        let cube_cpu = registry.get("cube").unwrap();
        let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

        let mut world = World::new();
        world.spawn((
            Transform::default(),
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform::default(),
            Visibility(false),
            Name("hidden".into()),
        ));

        let batches = extract_render_batches(&world);
        assert!(batches.is_empty());
    }

    #[test]
    fn extract_does_not_instance_batch_different_extended_materials() {
        let device = headless_device();
        let registry = BuiltinMeshRegistry::new();
        let cube_cpu = registry.get("cube").unwrap();
        let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

        let normal_mapped = MaterialUniform {
            normal_scale: 0.5,
            normal_texture_id: 11,
            ..MaterialUniform::default()
        };

        let mut world = World::new();
        for (name, material) in [
            ("plain", MaterialUniform::default()),
            ("normal", normal_mapped),
        ] {
            world.spawn((
                Transform::default(),
                MeshHandle::new(
                    cube_gpu.clone(),
                    crate::ecs::components::MeshSource::Builtin("cube".into()),
                    "cube",
                ),
                material,
                Visibility::default(),
                Name(name.into()),
            ));
        }

        let batches = extract_render_batches(&world);
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn extract_optional_data_is_empty_by_default() {
        let world = World::new();
        let optional = extract_optional_pass_data(&world);
        assert_eq!(optional, OptionalPassData::default());
    }

    #[test]
    fn extract_optional_data_finds_all_components() {
        let mut world = World::new();
        world.spawn((default_terrain(),));
        world.spawn((Water {
            config: WaterConfig::default(),
            dudv_texture: None,
            normal_texture: None,
        },));
        world.spawn((Atmosphere {
            config: AtmosphereConfig::default(),
        },));
        world.spawn((Clouds {
            config: CloudConfig::default(),
        },));
        world.spawn((GodRay {
            config: GodRayConfig::default(),
        },));

        let optional = extract_optional_pass_data(&world);
        assert!(optional.terrain.is_some());
        assert!(optional.water.is_some());
        assert!(optional.atmosphere.is_some());
        assert!(optional.clouds.is_some());
        assert!(optional.god_ray.is_some());
    }
}
