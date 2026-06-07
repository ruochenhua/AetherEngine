//! Extract phase: ECS World → GPU-ready render batches.
//!
//! Runs every frame before the render scheduler. Queries the ECS World for
//! renderable entities and produces `Vec<RenderBatch>` that the deferred
//! passes consume. Decouples ECS access from GPU command encoding.

use crate::asset::mesh::GpuMesh;
use crate::ecs::components::{MeshHandle, Transform, Visibility};
use crate::ecs::World;
use crate::renderer::renderable::MaterialUniform;
use glam::Mat4;
use std::sync::Arc;

/// Per-instance data within a batch.
///
/// Minimal for Phase 3 (one instance per batch). Fields will expand
/// in Phase 4/5 for true instancing.
#[derive(Clone, Debug)]
pub struct InstanceData {
    /// World-space model matrix.
    pub model: Mat4,
    /// Entity ID for picking feedback (packed to u32).
    pub entity_id: u32,
}

/// A group of instances sharing the same mesh and material.
///
/// Phase 3: each batch typically contains exactly one instance because
/// entities are not deduplicated by mesh/material. True batching will
/// be added when instancing is implemented.
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
/// and produces a flat `Vec<RenderBatch>` (one batch per visible entity).
pub fn extract_render_batches(world: &World) -> Vec<RenderBatch> {
    let mut batches = Vec::with_capacity(world.len() as usize);

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

        batches.push(RenderBatch {
            mesh: mesh_handle.mesh.clone(),
            material: *material,
            instances: vec![InstanceData {
                model,
                entity_id: entity.to_bits().get() as u32,
            }],
        });
    }

    batches
}
