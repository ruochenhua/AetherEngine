//! Terrain geometry generation.
//!
//! Produces chunked LOD meshes: a regular grid of vertices displaced by a
//! height function, with optional skirts to hide LOD cracks.

use crate::asset::mesh::CpuMesh;
use crate::math::{Aabb, Vec3};
use crate::scene::TerrainSource;
use noise::{Fbm, NoiseFn, Perlin};

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

/// Height function using FBM Perlin noise.
pub struct PerlinHeight {
    fbm: Fbm<Perlin>,
    amplitude: f32,
    exponent: f32,
}

impl PerlinHeight {
    /// Create a new Perlin height source.
    pub fn new(
        seed: u64,
        frequency: f32,
        amplitude: f32,
        octaves: u32,
        persistence: f32,
        lacunarity: f32,
        exponent: f32,
    ) -> Self {
        let mut fbm = Fbm::<Perlin>::new(seed as u32);
        fbm.frequency = frequency as f64;
        fbm.octaves = octaves as usize;
        fbm.persistence = persistence as f64;
        fbm.lacunarity = lacunarity as f64;
        Self {
            fbm,
            amplitude,
            exponent,
        }
    }
}

impl HeightFunction for PerlinHeight {
    fn sample(&self, x: f32, z: f32) -> f32 {
        let v = self.fbm.get([x as f64, z as f64]) as f32;
        let shaped = if v == 0.0 {
            0.0
        } else {
            v.signum() * v.abs().powf(self.exponent)
        };
        shaped * self.amplitude
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
    offset: (f32, f32),
) -> CpuMesh {
    let segments = segments.max(1);
    let verts_per_side = segments + 1;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    let half_size = size * 0.5;
    let step = size / segments as f32;

    // Helper to push a vertex. Sample height in world space by adding the chunk
    // origin offset so adjacent chunks share edge heights seamlessly.
    let mut add_vertex = |x: f32, z: f32, is_skirt: bool| {
        let wx = x + offset.0;
        let wz = z + offset.1;
        let y = if is_skirt {
            height.sample(wx, wz) - size * 0.03
        } else {
            height.sample(wx, wz)
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

    // Compute smooth normals from the height function gradient. Sampling in world
    // space guarantees that adjacent chunks produce matching edge normals, avoiding
    // the faceted shading seams you get when each chunk recomputes normals locally.
    compute_smooth_normals(&mut normals, &positions, offset, step, height);

    CpuMesh {
        positions,
        normals,
        uvs,
        tangents: Vec::new(),
        indices: generate_grid_indices(verts_per_side, with_skirt),
        submeshes: Vec::new(),
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

/// Compute smooth normals by finite-differencing the height function in world
/// space. This keeps normals consistent across chunk boundaries and removes the
/// faceted grid lines caused by per-chunk face-normal averaging.
fn compute_smooth_normals(
    normals: &mut [[f32; 3]],
    positions: &[[f32; 3]],
    offset: (f32, f32),
    step: f32,
    height: &dyn HeightFunction,
) {
    let two_step = step * 2.0;
    for (n, p) in normals.iter_mut().zip(positions.iter()) {
        let wx = p[0] + offset.0;
        let wz = p[2] + offset.1;

        let h_l = height.sample(wx - step, wz);
        let h_r = height.sample(wx + step, wz);
        let h_d = height.sample(wx, wz - step);
        let h_u = height.sample(wx, wz + step);

        let dh_dx = (h_r - h_l) / two_step;
        let dh_dz = (h_u - h_d) / two_step;

        let v = Vec3::new(-dh_dx, 1.0, -dh_dz).normalize();
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
        TerrainSource::Perlin {
            seed,
            frequency,
            amplitude,
            octaves,
            persistence,
            lacunarity,
            exponent,
        } => Box::new(PerlinHeight::new(
            *seed,
            *frequency,
            *amplitude,
            *octaves,
            *persistence,
            *lacunarity,
            *exponent,
        )),
    }
}

/// Generate LOD meshes for a single chunk.
pub fn generate_chunk_lod_meshes(
    max_lod: u32,
    base_size: f32,
    height: &dyn HeightFunction,
    offset: (f32, f32),
) -> Vec<CpuMesh> {
    let mut meshes = Vec::with_capacity((max_lod + 1) as usize);
    for lod in 0..=max_lod {
        // Each LOD halves the segment count.
        let segments = 4u32.max(64 >> lod);
        let with_skirt = lod != max_lod;
        meshes.push(generate_chunk_mesh(
            segments, base_size, height, with_skirt, offset,
        ));
    }
    meshes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_mesh_has_expected_vertex_count() {
        let height = ProceduralHeight::new(0, 0.1, 1.0);
        let mesh = generate_chunk_mesh(4, 64.0, &height, false, (0.0, 0.0));
        assert_eq!(mesh.positions.len(), 25);
        assert_eq!(mesh.indices.len(), 4 * 4 * 6);
    }

    #[test]
    fn chunk_mesh_with_skirt_is_larger() {
        let height = ProceduralHeight::new(0, 0.1, 1.0);
        let mesh = generate_chunk_mesh(4, 64.0, &height, true, (0.0, 0.0));
        let no_skirt = generate_chunk_mesh(4, 64.0, &height, false, (0.0, 0.0));
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
        let mesh = generate_chunk_mesh(4, 64.0, &height, false, (0.0, 0.0));
        let aabb = mesh.compute_aabb();
        assert!(aabb.min.x <= -31.0);
        assert!(aabb.max.x >= 31.0);
        assert!(aabb.min.z <= -31.0);
        assert!(aabb.max.z >= 31.0);
    }

    #[test]
    fn perlin_height_is_bounded() {
        let height = PerlinHeight::new(42, 0.01, 32.0, 4, 0.5, 2.0, 1.0);
        let h = height.sample(100.0, 200.0);
        assert!(h.abs() <= 32.0 * 1.1);
    }

    #[test]
    fn height_function_from_perlin_source() {
        let source = TerrainSource::Perlin {
            seed: 7,
            frequency: 0.02,
            amplitude: 16.0,
            octaves: 3,
            persistence: 0.5,
            lacunarity: 2.0,
            exponent: 1.0,
        };
        let height = height_function_from_source(&source);
        let _ = height.sample(0.0, 0.0);
    }
}
