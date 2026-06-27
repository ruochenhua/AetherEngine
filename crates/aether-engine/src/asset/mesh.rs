use super::Asset;
use crate::math::Aabb;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::path::Path;
use std::sync::Arc;

/// CPU-side PBR material description used while loading models.
///
/// This is a simplified material representation that captures the fields the
/// deferred PBR pipeline currently supports. It is populated by OBJ/ glTF
/// loaders and then converted into a [`MaterialUniform`](crate::renderer::renderable::MaterialUniform)
/// when spawning scene objects.
#[derive(Debug, Clone, Default)]
pub struct CpuMaterial {
    /// Human-readable material name.
    pub name: String,
    /// Base color multiplier.
    pub base_color: [f32; 4],
    /// Metallic factor.
    pub metallic: f32,
    /// Roughness factor.
    pub roughness: f32,
    /// Path to the albedo/base-color texture, relative to the project root.
    pub albedo_texture: Option<String>,
}

/// A contiguous range of indices in a [`CpuMesh`] that uses a specific material.
#[derive(Debug, Clone)]
pub struct CpuSubmesh {
    /// Human-readable submesh name (object/group/primitive name).
    pub name: String,
    /// Offset into [`CpuMesh::indices`].
    pub index_offset: usize,
    /// Number of indices.
    pub index_count: usize,
    /// Material to use for this range.
    pub material: CpuMaterial,
}

/// CPU-side mesh data.
#[derive(Debug, Clone)]
pub struct CpuMesh {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals.
    pub normals: Vec<[f32; 3]>,
    /// Vertex UV coordinates.
    pub uvs: Vec<[f32; 2]>,
    /// Vertex tangents (for normal mapping).
    pub tangents: Vec<[f32; 4]>,
    /// Index data.
    pub indices: Vec<u32>,
    /// Optional material submeshes. When empty the whole mesh uses the
    /// material assigned by the scene.
    pub submeshes: Vec<CpuSubmesh>,
}

impl CpuMesh {
    /// Compute the axis-aligned bounding box from vertex positions.
    pub fn compute_aabb(&self) -> Aabb {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in &self.positions {
            let v = Vec3::from_array(*p);
            min = min.min(v);
            max = max.max(v);
        }
        Aabb::new(min, max)
    }

    /// Create a cube mesh (1x1x1, centered at origin).
    pub fn cube() -> Self {
        // Cube vertices: 6 faces x 4 vertices = 24 vertices (no sharing for flat normals)
        let positions = vec![
            // Front face (+Z)
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
            // Back face (-Z)
            [0.5, -0.5, -0.5],
            [-0.5, -0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
            // Top face (+Y)
            [-0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            // Bottom face (-Y)
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [-0.5, -0.5, 0.5],
            // Right face (+X)
            [0.5, -0.5, 0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 0.5],
            // Left face (-X)
            [-0.5, -0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [-0.5, 0.5, -0.5],
        ];

        let normals = vec![
            // Front
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            // Back
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            // Top
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            // Bottom
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            // Right
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            // Left
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];

        let uvs = vec![
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Front
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Back
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Top
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Bottom
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Right
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0], // Left
        ];

        let indices: Vec<u32> = (0..6)
            .flat_map(|face| {
                let base = face * 4;
                vec![base, base + 1, base + 2, base, base + 2, base + 3]
            })
            .collect();

        Self {
            positions,
            normals,
            uvs,
            tangents: Vec::new(),
            indices,
            submeshes: Vec::new(),
        }
    }

    /// Create a sphere mesh (UV sphere, default 32 segments).
    pub fn sphere(segments: u32) -> Self {
        let segments = segments.max(3);
        let rings = segments;
        let sectors = segments;

        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        for r in 0..=rings {
            let theta = std::f32::consts::PI * (r as f32) / (rings as f32);
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            for s in 0..=sectors {
                let phi = 2.0 * std::f32::consts::PI * (s as f32) / (sectors as f32);
                let sin_phi = phi.sin();
                let cos_phi = phi.cos();

                let x = sin_theta * cos_phi;
                let y = cos_theta;
                let z = sin_theta * sin_phi;

                positions.push([x * 0.5, y * 0.5, z * 0.5]);
                normals.push([x, y, z]);
                uvs.push([s as f32 / sectors as f32, r as f32 / rings as f32]);
            }
        }

        for r in 0..rings {
            for s in 0..sectors {
                let base = r * (sectors + 1) + s;
                indices.push(base + sectors + 1);
                indices.push(base);
                indices.push(base + 1);
                indices.push(base + sectors + 1);
                indices.push(base + 1);
                indices.push(base + sectors + 2);
            }
        }

        Self {
            positions,
            normals,
            uvs,
            tangents: Vec::new(),
            indices,
            submeshes: Vec::new(),
        }
    }

    /// Create a quad mesh.
    pub fn quad() -> Self {
        let positions = vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ];
        let normals = vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let uvs = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let indices = vec![0, 1, 2, 0, 2, 3];

        Self {
            positions,
            normals,
            uvs,
            tangents: Vec::new(),
            indices,
            submeshes: Vec::new(),
        }
    }

    /// Y-up horizontal plane in XZ plane (normal = +Y, no rotation needed).
    pub fn plane() -> Self {
        let positions = vec![
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
        ];
        let normals = vec![
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices = vec![0, 3, 2, 0, 2, 1];

        Self {
            positions,
            normals,
            uvs,
            tangents: Vec::new(),
            indices,
            submeshes: Vec::new(),
        }
    }

    /// Convert to interleaved vertex data for GPU upload.
    pub fn to_vertices(&self) -> Vec<Vertex> {
        let count = self.positions.len();
        let mut vertices = Vec::with_capacity(count);
        for i in 0..count {
            vertices.push(Vertex {
                position: self.positions[i],
                normal: self.normals[i],
                uv: self.uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                tangent: self
                    .tangents
                    .get(i)
                    .copied()
                    .unwrap_or([1.0, 0.0, 0.0, 1.0]),
            });
        }
        vertices
    }
}

impl Asset for CpuMesh {
    fn load(path: &Path) -> anyhow::Result<Self> {
        crate::asset::loaders::load_mesh(path)
    }
}

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
        use wgpu::util::DeviceExt;

        let vertices = cpu.to_vertices();
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

        let aabb = cpu.compute_aabb();

        Self {
            vertex_buffer,
            index_buffer,
            index_offset: 0,
            index_count,
            vertex_count: vertices.len() as u32,
            aabb,
        }
    }

    /// Create a view into a contiguous index range of an existing GPU mesh.
    ///
    /// The returned mesh shares the same vertex/index buffers but draws only
    /// `index_count` indices starting at `index_offset`.
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
