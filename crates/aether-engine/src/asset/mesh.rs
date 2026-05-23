use super::Asset;
use bytemuck::{Pod, Zeroable};
use std::path::Path;

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
}

impl CpuMesh {
    /// Create a simple cube mesh.
    pub fn cube() -> Self {
        // TODO: Implement cube mesh generation
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            indices: Vec::new(),
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
        let uvs = vec![
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];

        Self {
            positions,
            normals,
            uvs,
            tangents: Vec::new(),
            indices,
        }
    }
}

impl Asset for CpuMesh {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "obj" => load_obj(path),
            "gltf" | "glb" => load_gltf(path),
            _ => anyhow::bail!("Unsupported mesh format: {}", ext),
        }
    }
}

fn load_obj(_path: &Path) -> anyhow::Result<CpuMesh> {
    // TODO: Implement OBJ loading
    anyhow::bail!("OBJ loading not yet implemented")
}

fn load_gltf(_path: &Path) -> anyhow::Result<CpuMesh> {
    // TODO: Implement GLTF mesh loading
    anyhow::bail!("GLTF mesh loading not yet implemented")
}

/// GPU mesh representation.
#[derive(Debug)]
pub struct GpuMesh {
    /// Vertex buffer.
    pub vertex_buffer: wgpu::Buffer,
    /// Index buffer (optional).
    pub index_buffer: Option<wgpu::Buffer>,
    /// Number of indices.
    pub index_count: u32,
    /// Number of vertices.
    pub vertex_count: u32,
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
