//! Extract phase: ECS World → GPU-ready render batches.
//!
//! Runs every frame before the render scheduler. Queries the ECS World for
//! renderable entities and produces `Vec<RenderBatch>` that the deferred
//! passes consume. Decouples ECS access from GPU command encoding.

use crate::asset::mesh::{GpuMesh, InstanceData};
use crate::ecs::components::{MeshHandle, Transform, Visibility};
use crate::ecs::World;
use crate::math::{CullingVisibility, Frustum, Mat4};
use crate::renderer::renderable::MaterialUniform;
use std::collections::HashMap;
use std::sync::Arc;

/// Grouping key for instanced batches.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    mesh: *const GpuMesh,
    material: MaterialBits,
}

/// Bit-level representation of `MaterialUniform` so it can be hashed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MaterialBits {
    albedo: [u32; 4],
    roughness: u32,
    metallic: u32,
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
    /// Instances to draw.
    pub instances: Vec<InstanceData>,
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

    for (entity, transform, mesh_handle, material, visibility) in world
        .query::<(
            hecs::Entity,
            &Transform,
            &MeshHandle,
            &MaterialUniform,
            &Visibility,
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
        };
        batches
            .entry(key)
            .or_insert_with(|| RenderBatch {
                mesh: mesh_handle.mesh.clone(),
                material: *material,
                instances: Vec::new(),
            })
            .instances
            .push(instance);
    }

    batches.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::mesh::GpuMesh;
    use crate::asset::registry::BuiltinMeshRegistry;
    use crate::ecs::components::{Name, Visibility};
    use crate::ecs::World;
    use crate::math::{Frustum, Vec3};
    use crate::renderer::renderable::MaterialUniform;
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
            MeshHandle::new(cube_gpu.clone(), "cube"),
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
            MeshHandle::new(cube_gpu.clone(), "cube"),
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
            MeshHandle::new(cube_gpu.clone(), "cube"),
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
            MeshHandle::new(cube_gpu.clone(), "cube"),
            MaterialUniform::default(),
            Visibility(false),
            Name("hidden".into()),
        ));

        let batches = extract_render_batches(&world);
        assert!(batches.is_empty());
    }
}
