//! Terrain geometry generation.
//!
//! Produces chunked LOD meshes: a regular grid of vertices displaced by a
//! height function, with optional skirts to hide LOD cracks.

use crate::asset::mesh::CpuMesh;
use crate::math::{Aabb, Vec3};
use crate::scene::TerrainSource;

/// A single terrain chunk at a specific LOD level.
#[derive(Debug, Clone)]
pub struct TerrainChunkMesh {
    /// CPU mesh for this LOD level.
    pub mesh: CpuMesh,
    /// World-space axis-aligned bounds before transform.
    pub local_aabb: Aabb,
    /// Chunk grid coordinates (col, row).
    pub coord: (i32, i32),
    /// LOD level.
    pub lod: u32,
}

/// Height sampling function for terrain vertices.
pub trait HeightFunction: Send + Sync {
    /// Sample height at a given world XZ position.
    fn sample(&self, x: f32, z: f32) -> f32;
}

/// Procedural height function using layered sine waves.
pub struct ProceduralHeight {
    seed: u64,
    frequency: f32,
    amplitude: f32,
}

impl ProceduralHeight {
    /// Create a new procedural height source.
    pub fn new(seed: u64, frequency: f32, amplitude: f32) -> Self {
        Self {
            seed,
            frequency,
            amplitude,
        }
    }
}

impl HeightFunction for ProceduralHeight {
    fn sample(&self, x: f32, z: f32) -> f32 {
        // Simple layered sine noise for testing visuals.
        // Replace with a proper noise library in production.
        let fx = x * self.frequency;
        let fz = z * self.frequency;
        let h0 = (fx + self.seed as f32).sin() * (fz + self.seed as f32).cos();
        let h1 = (fx * 2.3 + fz * 1.7 + self.seed as f32 * 0.5).sin() * 0.5;
        let h2 = (fx * 0.7 - fz * 2.1 + self.seed as f32 * 0.25).cos() * 0.25;
        (h0 + h1 + h2) * self.amplitude
    }
}

/// Generate a flat grid mesh with the given vertex count and world-space size.
///
/// `segments` is the number of quads along each edge. The mesh is centered at
/// the origin in XZ and spans `size` world units.
pub fn generate_chunk_mesh(
    segments: u32,
    size: f32,
    height: &dyn HeightFunction,
    with_skirt: bool,
) -> CpuMesh {
    let segments = segments.max(1);
    let verts_per_side = segments + 1;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    let half_size = size * 0.5;
    let step = size / segments as f32;

    // Helper to push a vertex.
    let mut add_vertex = |x: f32, z: f32, is_skirt: bool| {
        let y = if is_skirt {
            height.sample(x, z) - size * 0.15
        } else {
            height.sample(x, z)
        };
        positions.push([x, y, z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([(x + half_size) / size, (z + half_size) / size]);
    };

    if with_skirt {
        // Interior grid + one-cell-wide skirt border.
        let skirt_segments = segments + 2;
        let skirt_verts_per_side = skirt_segments + 1;
        let skirt_half_size = half_size + step;
        for row in 0..skirt_verts_per_side {
            for col in 0..skirt_verts_per_side {
                let x = -skirt_half_size + col as f32 * step;
                let z = -skirt_half_size + row as f32 * step;
                let is_border =
                    row == 0 || row == skirt_segments || col == 0 || col == skirt_segments;
                add_vertex(x, z, is_border);
            }
        }
    } else {
        for row in 0..verts_per_side {
            for col in 0..verts_per_side {
                let x = -half_size + col as f32 * step;
                let z = -half_size + row as f32 * step;
                add_vertex(x, z, false);
            }
        }
    }

    // Compute flat normals from face averages.
    recompute_normals(&mut normals, &positions);

    CpuMesh {
        positions,
        normals,
        uvs,
        tangents: Vec::new(),
        indices: generate_grid_indices(verts_per_side, with_skirt),
    }
}

fn generate_grid_indices(verts_per_side: u32, with_skirt: bool) -> Vec<u32> {
    if !with_skirt {
        let mut indices = Vec::with_capacity((verts_per_side * verts_per_side * 6) as usize);
        for row in 0..verts_per_side - 1 {
            for col in 0..verts_per_side - 1 {
                let a = row * verts_per_side + col;
                let b = a + 1;
                let c = (row + 1) * verts_per_side + col;
                let d = c + 1;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        return indices;
    }

    let skirt_segments = verts_per_side + 1;
    let skirt_verts_per_side = skirt_segments + 1;
    let mut indices = Vec::with_capacity((skirt_segments * skirt_segments * 6) as usize);
    for row in 0..skirt_segments {
        for col in 0..skirt_segments {
            let a = row * skirt_verts_per_side + col;
            let b = a + 1;
            let c = (row + 1) * skirt_verts_per_side + col;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    indices
}

fn recompute_normals(normals: &mut [[f32; 3]], positions: &[[f32; 3]]) {
    for n in normals.iter_mut() {
        *n = [0.0, 0.0, 0.0];
    }
    // Assume a regular grid. Accumulate face normals per vertex.
    let count = positions.len();
    let side = (count as f32).sqrt() as usize;
    if side * side != count {
        // Fallback for non-square grids.
        for n in normals.iter_mut() {
            *n = [0.0, 1.0, 0.0];
        }
        return;
    }

    for row in 0..side - 1 {
        for col in 0..side - 1 {
            let i0 = row * side + col;
            let i1 = i0 + 1;
            let i2 = (row + 1) * side + col;
            let i3 = i2 + 1;

            let p0 = Vec3::from_array(positions[i0]);
            let p1 = Vec3::from_array(positions[i1]);
            let p2 = Vec3::from_array(positions[i2]);
            let p3 = Vec3::from_array(positions[i3]);

            let n0 = (p2 - p0).cross(p1 - p0).normalize();
            let n1 = (p1 - p2).cross(p3 - p2).normalize();

            for idx in [i0, i1, i2] {
                let v = Vec3::from_array(normals[idx]) + n0;
                normals[idx] = v.to_array();
            }
            for idx in [i1, i2, i3] {
                let v = Vec3::from_array(normals[idx]) + n1;
                normals[idx] = v.to_array();
            }
        }
    }

    for n in normals.iter_mut() {
        let v = Vec3::from_array(*n).normalize();
        *n = if v.is_nan() {
            [0.0, 1.0, 0.0]
        } else {
            v.to_array()
        };
    }
}

/// Build a height function from a `TerrainSource`.
pub fn height_function_from_source(source: &TerrainSource) -> Box<dyn HeightFunction> {
    match source {
        TerrainSource::Heightmap(_path) => {
            // Phase 5 heightmap loading: for now fall back to procedural.
            // A future issue will implement image-based heightmap sampling.
            Box::new(ProceduralHeight::new(0, 0.02, 32.0))
        }
        TerrainSource::Procedural {
            seed,
            frequency,
            amplitude,
        } => Box::new(ProceduralHeight::new(*seed, *frequency, *amplitude)),
    }
}

/// Generate LOD meshes for a single chunk.
pub fn generate_chunk_lod_meshes(
    max_lod: u32,
    base_size: f32,
    height: &dyn HeightFunction,
) -> Vec<CpuMesh> {
    let mut meshes = Vec::with_capacity((max_lod + 1) as usize);
    for lod in 0..=max_lod {
        // Each LOD halves the segment count.
        let segments = 4u32.max(64 >> lod);
        let with_skirt = lod != max_lod;
        meshes.push(generate_chunk_mesh(segments, base_size, height, with_skirt));
    }
    meshes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_mesh_has_expected_vertex_count() {
        let height = ProceduralHeight::new(0, 0.1, 1.0);
        let mesh = generate_chunk_mesh(4, 64.0, &height, false);
        assert_eq!(mesh.positions.len(), 25);
        assert_eq!(mesh.indices.len(), 4 * 4 * 6);
    }

    #[test]
    fn chunk_mesh_with_skirt_is_larger() {
        let height = ProceduralHeight::new(0, 0.1, 1.0);
        let mesh = generate_chunk_mesh(4, 64.0, &height, true);
        let no_skirt = generate_chunk_mesh(4, 64.0, &height, false);
        assert!(mesh.positions.len() > no_skirt.positions.len());
    }

    #[test]
    fn procedural_height_is_bounded() {
        let height = ProceduralHeight::new(42, 0.05, 10.0);
        let h = height.sample(100.0, 200.0);
        assert!(h.abs() <= 10.0 * 1.75); // sum of amplitudes
    }

    #[test]
    fn chunk_aabb_computes_from_positions() {
        let height = ProceduralHeight::new(0, 0.1, 10.0);
        let mesh = generate_chunk_mesh(4, 64.0, &height, false);
        let aabb = mesh.compute_aabb();
        assert!(aabb.min.x <= -31.0);
        assert!(aabb.max.x >= 31.0);
        assert!(aabb.min.z <= -31.0);
        assert!(aabb.max.z >= 31.0);
    }
}
