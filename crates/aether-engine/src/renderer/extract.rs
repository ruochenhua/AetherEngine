//! Extract phase: ECS World → GPU-ready render batches.
//!
//! Runs every frame before the render scheduler. Queries the ECS World for
//! renderable entities and produces `Vec<RenderBatch>` that the deferred
//! passes consume. Decouples ECS access from GPU command encoding.

use crate::asset::mesh::{GpuMesh, InstanceData};
use crate::ecs::components::{MeshHandle, Transform, Visibility};
use crate::ecs::World;
use crate::renderer::renderable::MaterialUniform;
use glam::Mat4;
use std::sync::Arc;

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
    let mut batches: Vec<RenderBatch> = Vec::with_capacity(world.len() as usize);

    for (entity, transform, mesh_handle, material, visibility) in world
        .query::<(hecs::Entity, &Transform, &MeshHandle, &MaterialUniform, &Visibility)>()
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

        let instance = InstanceData {
            model_matrix: model.to_cols_array_2d(),
            entity_id: entity.to_bits().get() as u32,
            _pad: [0; 3],
        };

        if let Some(batch) = batches.iter_mut().find(|b| {
            Arc::ptr_eq(&b.mesh, &mesh_handle.mesh) && b.material == *material
        }) {
            batch.instances.push(instance);
        } else {
            batches.push(RenderBatch {
                mesh: mesh_handle.mesh.clone(),
                material: *material,
                instances: vec![instance],
            });
        }
    }

    batches
}
