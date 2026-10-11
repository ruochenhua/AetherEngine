use super::Asset;
use crate::math::Aabb;
use glam::Vec3;
use std::path::Path;

mod gpu;
pub use gpu::{GpuMesh, InstanceData, Vertex};

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
    /// Source glTF node for this imported primitive, when available.
    pub source_node_index: Option<usize>,
    /// Source mesh primitive for this imported primitive, when available.
    pub source_primitive_index: Option<usize>,
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
        let mut normals = self.normals.clone();
        if normals.len() != count {
            crate::asset::loaders::compute_smooth_normals(
                &self.positions,
                &self.indices,
                &mut normals,
            );
        }
        let mut vertices = Vec::with_capacity(count);
        for i in 0..count {
            vertices.push(Vertex {
                position: self.positions[i],
                normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                uv: self.uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                tangent: self
                    .tangents
                    .get(i)
                    .copied()
                    .unwrap_or([1.0, 0.0, 0.0, 1.0]),
                joints: [0; 4],
                weights: [0.0; 4],
            });
        }
        vertices
    }

    /// Convert to vertex data with four glTF skin influences per vertex.
    pub fn to_skinned_vertices(
        &self,
        skin: &[crate::asset::gltf_document::JointWeights],
    ) -> Option<Vec<Vertex>> {
        if skin.len() != self.positions.len() {
            return None;
        }
        let mut vertices = self.to_vertices();
        for (vertex, influence) in vertices.iter_mut().zip(skin) {
            vertex.joints = influence.joints.map(u32::from);
            vertex.weights = influence.weights;
        }
        Some(vertices)
    }
}

impl Asset for CpuMesh {
    fn load(path: &Path) -> anyhow::Result<Self> {
        crate::asset::loaders::load_mesh(path)
    }
}

#[cfg(test)]
#[path = "mesh_tests.rs"]
mod tests;
