//! Draw-batch preparation for extracted animated geometry.

use super::{opaque_batches, RenderBatch};
use crate::animation::{AnimationFrame, AnimationRenderItem};
use crate::asset::gltf_document::MAX_SKELETON_JOINTS;
use crate::asset::mesh::{GpuMesh, InstanceData};
use bytemuck::cast_slice;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Replace static instances for successfully skinned entities with their deformed draw geometry.
pub fn animation_aware_batches(
    device: &wgpu::Device,
    source: &Arc<[RenderBatch]>,
    animation: Option<&AnimationFrame>,
) -> Vec<RenderBatch> {
    let Some(animation) = animation else {
        return opaque_batches(source).iter().cloned().collect();
    };
    let skinned = animation
        .items
        .iter()
        .filter_map(|item| match item {
            AnimationRenderItem::Skinned(item) => Some(item),
            AnimationRenderItem::StaticFallback(_) => None,
        })
        .collect::<Vec<_>>();
    if skinned.is_empty() {
        return opaque_batches(source).iter().cloned().collect();
    }

    let skinned_entities = skinned
        .iter()
        .map(|item| item.entity_bits as u32)
        .collect::<std::collections::HashSet<_>>();
    let mut batches = opaque_batches(source).iter().cloned().collect::<Vec<_>>();
    for batch in &mut batches {
        batch
            .instances
            .retain(|instance| !skinned_entities.contains(&instance.entity_id));
    }

    for item in skinned {
        let entity_id = item.entity_bits as u32;
        let source_batch = source.iter().find(|batch| {
            batch
                .instances
                .iter()
                .any(|instance| instance.entity_id == entity_id)
        });
        batches.push(RenderBatch {
            mesh: Arc::new(
                GpuMesh::from_cpu_skinned(device, &item.source_mesh, &item.weights)
                    .unwrap_or_else(|| GpuMesh::from_cpu(device, &item.source_mesh)),
            ),
            material: source_batch.map_or_else(
                crate::renderer::renderable::MaterialUniform::default,
                |batch| batch.material,
            ),
            albedo_texture: source_batch.and_then(|batch| batch.albedo_texture.clone()),
            normal_texture: source_batch.and_then(|batch| batch.normal_texture.clone()),
            orm_texture: source_batch.and_then(|batch| batch.orm_texture.clone()),
            emissive_texture: source_batch.and_then(|batch| batch.emissive_texture.clone()),
            transparent_material: None,
            skin_palette: Some(create_skin_palette_buffer(
                device,
                item.palette.matrices.as_ref(),
            )),
            instances: vec![InstanceData {
                model_matrix: item.world_transform.to_cols_array_2d(),
                entity_id,
                _pad: [0; 3],
            }],
        });
    }
    batches
}

fn create_skin_palette_buffer(device: &wgpu::Device, matrices: &[glam::Mat4]) -> Arc<wgpu::Buffer> {
    let mut padded = [glam::Mat4::IDENTITY.to_cols_array_2d(); MAX_SKELETON_JOINTS];
    for (output, matrix) in padded
        .iter_mut()
        .zip(matrices.iter().take(MAX_SKELETON_JOINTS))
    {
        *output = matrix.to_cols_array_2d();
    }
    Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Skinned Mesh Joint Palette"),
            contents: cast_slice(&padded),
            usage: wgpu::BufferUsages::STORAGE,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::mesh::CpuMesh;
    use crate::math::{Aabb, Mat4, Vec3};
    use crate::renderer::renderable::MaterialUniform;

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
    fn animation_batch_replaces_static_entity_geometry_with_skinned_geometry() {
        let device = headless_device();
        let cpu_mesh = CpuMesh::cube();
        let gpu_mesh = Arc::new(GpuMesh::from_cpu(&device, &cpu_mesh));
        let batch = RenderBatch {
            mesh: gpu_mesh,
            material: MaterialUniform::default(),
            albedo_texture: None,
            normal_texture: None,
            orm_texture: None,
            emissive_texture: None,
            transparent_material: None,
            skin_palette: None,
            instances: vec![
                InstanceData {
                    model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
                    entity_id: 1,
                    _pad: [0; 3],
                },
                InstanceData {
                    model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
                    entity_id: 2,
                    _pad: [0; 3],
                },
            ],
        };
        let batches: Arc<[RenderBatch]> = Arc::from([batch.clone()]);
        let animation = crate::animation::animation_frame_with_bounds(Aabb::new(
            Vec3::splat(-1.0),
            Vec3::splat(1.0),
        ));

        let prepared = animation_aware_batches(&device, &batches, Some(&animation));

        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].instances.len(), 1);
        assert_eq!(prepared[0].instances[0].entity_id, 2);
        assert_eq!(prepared[1].instances[0].entity_id, 1);
        assert_eq!(prepared[1].mesh.vertex_count, 2);
        assert_eq!(prepared[1].material, batch.material);
    }
}
