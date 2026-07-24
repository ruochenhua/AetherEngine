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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every index must reference an existing vertex — an off-by-one in index
    /// generation would otherwise only show up as GPU-side garbage.
    fn assert_indices_in_bounds(mesh: &CpuMesh) {
        let vertex_count = mesh.positions.len() as u32;
        let max_index = mesh.indices.iter().copied().max().unwrap_or(0);
        assert!(
            max_index < vertex_count,
            "index {max_index} out of bounds for {vertex_count} vertices"
        );
    }

    #[test]
    fn cube_has_24_vertices_and_36_indices() {
        let mesh = CpuMesh::cube();
        // 6 faces x 4 vertices (no sharing, flat normals), 6 faces x 2 triangles.
        assert_eq!(mesh.positions.len(), 24, "cube must have 6 faces x 4 vertices");
        assert_eq!(mesh.normals.len(), 24, "cube must have one normal per vertex");
        assert_eq!(mesh.uvs.len(), 24, "cube must have one uv per vertex");
        assert_eq!(mesh.indices.len(), 36, "cube must have 6 faces x 2 triangles x 3 indices");
        assert_indices_in_bounds(&mesh);
    }

    #[test]
    fn cube_aabb_is_unit_cube_centered_at_origin() {
        // cube() is documented as 1x1x1 centered at the origin, so the AABB is
        // [-0.5, 0.5]^3 (quad/plane are the ones spanning [-1, 1]).
        let aabb = CpuMesh::cube().compute_aabb();
        assert_eq!(
            aabb,
            Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5)),
            "cube AABB must be the 1x1x1 box centered at the origin"
        );
    }

    #[test]
    fn sphere_counts_follow_segment_formula_and_grow() {
        for segments in [8u32, 16, 32] {
            let mesh = CpuMesh::sphere(segments);
            let expected_vertices = ((segments + 1) * (segments + 1)) as usize;
            let expected_indices = (segments * segments * 6) as usize;
            assert_eq!(
                mesh.positions.len(),
                expected_vertices,
                "sphere({segments}) must have (rings + 1) * (sectors + 1) vertices"
            );
            assert_eq!(
                mesh.indices.len(),
                expected_indices,
                "sphere({segments}) must have rings * sectors * 6 indices"
            );
            assert_indices_in_bounds(&mesh);
        }
        assert!(
            CpuMesh::sphere(16).positions.len() > CpuMesh::sphere(8).positions.len(),
            "sphere vertex count must grow with segments"
        );
    }

    #[test]
    fn sphere_vertices_lie_on_radius() {
        let mesh = CpuMesh::sphere(32);
        for (i, p) in mesh.positions.iter().enumerate() {
            let distance = Vec3::from_array(*p).length();
            assert!(
                (distance - 0.5).abs() < 1e-4,
                "vertex {i} is {distance} from the origin, expected radius 0.5"
            );
        }
        // Normals are the unscaled sphere direction and must be unit length.
        for (i, n) in mesh.normals.iter().enumerate() {
            let length = Vec3::from_array(*n).length();
            assert!(
                (length - 1.0).abs() < 1e-4,
                "normal {i} has length {length}, expected 1.0"
            );
        }
    }

    #[test]
    fn sphere_segments_below_three_are_clamped() {
        let clamped = CpuMesh::sphere(1);
        let minimum = CpuMesh::sphere(3);
        assert_eq!(
            clamped.positions.len(),
            minimum.positions.len(),
            "sphere(1) must be clamped to the 3-segment minimum vertex count"
        );
        assert_eq!(
            clamped.indices.len(),
            minimum.indices.len(),
            "sphere(1) must be clamped to the 3-segment minimum index count"
        );
    }

    #[test]
    fn quad_is_z_facing_with_expected_counts() {
        let mesh = CpuMesh::quad();
        assert_eq!(mesh.positions.len(), 4, "quad must have 4 vertices");
        assert_eq!(mesh.indices.len(), 6, "quad must have 2 triangles x 3 indices");
        assert!(
            mesh.normals.iter().all(|n| *n == [0.0, 0.0, 1.0]),
            "quad normals must all be +Z"
        );
        assert_eq!(
            mesh.compute_aabb(),
            Aabb::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0)),
            "quad AABB must span [-1, 1] in XY at z = 0"
        );
        assert_indices_in_bounds(&mesh);
    }

    #[test]
    fn plane_is_y_up_with_expected_counts() {
        let mesh = CpuMesh::plane();
        assert_eq!(mesh.positions.len(), 4, "plane must have 4 vertices");
        assert_eq!(mesh.indices.len(), 6, "plane must have 2 triangles x 3 indices");
        assert!(
            mesh.normals.iter().all(|n| *n == [0.0, 1.0, 0.0]),
            "plane normals must all be +Y"
        );
        assert!(
            mesh.positions.iter().all(|p| p[1] == 0.0),
            "plane must lie in the XZ plane (y == 0)"
        );
        assert_eq!(
            mesh.compute_aabb(),
            Aabb::new(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 1.0)),
            "plane AABB must span [-1, 1] in XZ at y = 0"
        );
        assert_indices_in_bounds(&mesh);
    }

    #[test]
    fn compute_aabb_matches_manual_min_max() {
        let positions = vec![[1.5, -2.0, 0.25], [-3.0, 4.0, 1.0], [0.0, 0.0, -7.5]];
        let mesh = CpuMesh {
            positions: positions.clone(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
        };
        let aabb = mesh.compute_aabb();

        // Manual fold, independent of the implementation under test.
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &positions {
            for (axis, value) in p.iter().enumerate() {
                min[axis] = min[axis].min(*value);
                max[axis] = max[axis].max(*value);
            }
        }
        assert_eq!(
            aabb,
            Aabb::new(Vec3::from_array(min), Vec3::from_array(max)),
            "compute_aabb must agree with a manual per-axis min/max fold"
        );
        // Literal corner spot-checks so the test is not tautological.
        assert_eq!(aabb.min, Vec3::new(-3.0, -2.0, -7.5), "wrong AABB min corner");
        assert_eq!(aabb.max, Vec3::new(1.5, 4.0, 1.0), "wrong AABB max corner");
    }
}
