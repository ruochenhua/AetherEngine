use super::SkinnedVertex;
use crate::math::{Aabb, Vec3};
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// GPU buffers for one skinned primitive, shared by render consumers.
#[derive(Debug)]
pub struct GpuSkinnedMesh {
    /// Interleaved position, normal, UV, tangent, joint, and weight data.
    pub vertex_buffer: Arc<wgpu::Buffer>,
    /// Triangle indices, when the primitive is indexed.
    pub index_buffer: Option<Arc<wgpu::Buffer>>,
    /// Number of indices or vertices consumed by the draw.
    pub draw_count: u32,
    /// Axis-aligned bounds in primitive-local space.
    pub aabb: Aabb,
}

impl GpuSkinnedMesh {
    /// Upload a skinned primitive to the GPU.
    pub fn new(device: &wgpu::Device, vertices: &[SkinnedVertex], indices: &[u32]) -> Self {
        let vertex_buffer = Arc::new(device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Skinned Mesh Vertex Buffer"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));
        let index_buffer = (!indices.is_empty()).then(|| {
            Arc::new(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Skinned Mesh Index Buffer"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            )
        });
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in vertices {
            let position = Vec3::from_array(vertex.position);
            min = min.min(position);
            max = max.max(position);
        }
        Self {
            vertex_buffer,
            index_buffer,
            draw_count: if indices.is_empty() {
                vertices.len() as u32
            } else {
                indices.len() as u32
            },
            aabb: Aabb::new(min, max),
        }
    }
}
