use super::CpuMesh;
use crate::math::Aabb;
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

/// GPU mesh representation.
#[derive(Debug)]
pub struct GpuMesh {
    /// Vertex buffer.
    pub vertex_buffer: Arc<wgpu::Buffer>,
    /// Index buffer (optional).
    pub index_buffer: Option<Arc<wgpu::Buffer>>,
    /// Offset into the index buffer for the first draw index.
    pub index_offset: u32,
    /// Number of indices.
    pub index_count: u32,
    /// Number of vertices.
    pub vertex_count: u32,
    /// Axis-aligned bounding box in model space.
    pub aabb: Aabb,
}

impl GpuMesh {
    /// Upload a CPU mesh to GPU.
    pub fn from_cpu(device: &wgpu::Device, cpu: &CpuMesh) -> Self {
        Self::from_vertices(device, cpu, cpu.to_vertices())
    }

    /// Upload a CPU mesh with glTF skin influences attached to each vertex.
    pub fn from_cpu_skinned(
        device: &wgpu::Device,
        cpu: &CpuMesh,
        weights: &[crate::asset::gltf_document::JointWeights],
    ) -> Option<Self> {
        Some(Self::from_vertices(
            device,
            cpu,
            cpu.to_skinned_vertices(weights)?,
        ))
    }

    fn from_vertices(device: &wgpu::Device, cpu: &CpuMesh, vertices: Vec<Vertex>) -> Self {
        use wgpu::util::DeviceExt;
        let vertex_buffer = Arc::new(device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        let (index_buffer, index_count) = if cpu.indices.is_empty() {
            (None, vertices.len() as u32)
        } else {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Index Buffer"),
                contents: bytemuck::cast_slice(&cpu.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (Some(Arc::new(buffer)), cpu.indices.len() as u32)
        };

        Self {
            vertex_buffer,
            index_buffer,
            index_offset: 0,
            index_count,
            vertex_count: vertices.len() as u32,
            aabb: cpu.compute_aabb(),
        }
    }

    /// Create a view into a contiguous index range of an existing GPU mesh.
    pub fn submesh_view(parent: &GpuMesh, index_offset: u32, index_count: u32) -> Self {
        Self {
            vertex_buffer: Arc::clone(&parent.vertex_buffer),
            index_buffer: parent.index_buffer.as_ref().map(Arc::clone),
            index_offset,
            index_count,
            vertex_count: parent.vertex_count,
            aabb: parent.aabb,
        }
    }
}

/// Vertex layout for standard PBR mesh.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Position.
    pub position: [f32; 3],
    /// Normal.
    pub normal: [f32; 3],
    /// UV coordinates.
    pub uv: [f32; 2],
    /// Tangent (xyz) + handedness (w).
    pub tangent: [f32; 4],
    /// Four skin joint indices local to the primitive's palette.
    pub joints: [u32; 4],
    /// Four normalized skin influence weights.
    pub weights: [f32; 4],
}

impl Vertex {
    /// Describe the vertex buffer layout.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress
                        + std::mem::size_of::<[u32; 4]>() as wgpu::BufferAddress,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Per-instance vertex data for GPU instancing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InstanceData {
    /// World-space model matrix (column-major).
    pub model_matrix: [[f32; 4]; 4],
    /// Entity ID for picking feedback.
    pub entity_id: u32,
    /// Padding to 16-byte alignment.
    pub _pad: [u32; 3],
}

impl InstanceData {
    /// Describe the instance vertex buffer layout.
    pub fn instance_desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}
