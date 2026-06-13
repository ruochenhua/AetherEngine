//! Terrain LOD selection and chunk management.

use crate::math::{Aabb, CullingVisibility, Frustum, Vec3};

/// A terrain chunk with LOD selection state.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Chunk grid column.
    pub col: i32,
    /// Chunk grid row.
    pub row: i32,
    /// World-space center of the chunk (flat XZ center, Y = 0).
    pub center: Vec3,
    /// Chunk world-space size (square).
    pub size: f32,
    /// Currently selected LOD level.
    pub lod: u32,
    /// Maximum available LOD level.
    pub max_lod: u32,
}

impl Chunk {
    /// Create a chunk descriptor.
    pub fn new(col: i32, row: i32, center: Vec3, size: f32, max_lod: u32) -> Self {
        Self {
            col,
            row,
            center,
            size,
            lod: 0,
            max_lod,
        }
    }

    /// World-space axis-aligned bounds for this chunk.
    ///
    /// `height_min` and `height_max` are the estimated height range over the
    /// whole terrain; individual chunk AABBs could be tighter in Phase 5+.
    pub fn aabb(&self, height_min: f32, height_max: f32) -> Aabb {
        let half = self.size * 0.5;
        Aabb::new(
            Vec3::new(self.center.x - half, height_min, self.center.z - half),
            Vec3::new(self.center.x + half, height_max, self.center.z + half),
        )
    }

    /// Compute squared horizontal distance from a world point.
    pub fn distance_squared_to(&self, point: Vec3) -> f32 {
        let dx = self.center.x - point.x;
        let dz = self.center.z - point.z;
        dx * dx + dz * dz
    }

    /// Select LOD based on distance to camera.
    ///
    /// `lod_scale` controls how quickly LOD falls off. Larger values keep
    /// high LOD further away.
    pub fn select_lod(&mut self, camera_pos: Vec3, lod_scale: f32) {
        let dist_sq = self.distance_squared_to(camera_pos);
        let dist = dist_sq.sqrt();
        // Each LOD level covers roughly twice the distance of the next.
        let mut selected = self.max_lod;
        for lod in 0..=self.max_lod {
            let threshold = self.size * lod_scale * (1u32 << lod) as f32;
            if dist < threshold {
                selected = lod;
                break;
            }
        }
        self.lod = selected.min(self.max_lod);
    }
}

/// Build a grid of chunks covering the terrain extent.
pub fn build_chunk_grid(extent: f32, chunk_size: f32, max_lod: u32) -> Vec<Chunk> {
    let half_extent = extent;
    let chunks_per_side = ((extent * 2.0) / chunk_size).ceil() as i32;
    let mut chunks = Vec::new();

    for row in 0..chunks_per_side {
        for col in 0..chunks_per_side {
            let x = -half_extent + (col as f32 + 0.5) * chunk_size;
            let z = -half_extent + (row as f32 + 0.5) * chunk_size;
            let center = Vec3::new(x, 0.0, z);
            chunks.push(Chunk::new(col, row, center, chunk_size, max_lod));
        }
    }

    chunks
}

/// Filter chunks by frustum visibility and select their LOD.
pub fn cull_and_select_lod(
    chunks: &mut [Chunk],
    camera_pos: Vec3,
    frustum: &Frustum,
    height_min: f32,
    height_max: f32,
    lod_scale: f32,
) -> Vec<usize> {
    let mut visible = Vec::new();
    for (index, chunk) in chunks.iter_mut().enumerate() {
        let aabb = chunk.aabb(height_min, height_max);
        if aabb.intersects_frustum(frustum) == CullingVisibility::Invisible {
            continue;
        }
        chunk.select_lod(camera_pos, lod_scale);
        visible.push(index);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Mat4;

    #[test]
    fn chunk_aabb_center_matches() {
        let chunk = Chunk::new(0, 0, Vec3::new(10.0, 0.0, 20.0), 32.0, 4);
        let aabb = chunk.aabb(-10.0, 50.0);
        assert_eq!(aabb.center().x, 10.0);
        assert_eq!(aabb.center().z, 20.0);
    }

    #[test]
    fn lod_selects_higher_for_nearby_chunks() {
        let mut chunk = Chunk::new(0, 0, Vec3::ZERO, 64.0, 4);
        chunk.select_lod(Vec3::ZERO, 2.0);
        assert_eq!(chunk.lod, 0);

        let mut far_chunk = Chunk::new(0, 0, Vec3::ZERO, 64.0, 4);
        far_chunk.select_lod(Vec3::new(2000.0, 0.0, 0.0), 2.0);
        assert!(far_chunk.lod > 0);
    }

    #[test]
    fn chunk_grid_covers_extent() {
        let chunks = build_chunk_grid(256.0, 64.0, 4);
        assert_eq!(chunks.len(), 8 * 8);
        let first = &chunks[0];
        assert_eq!(first.center.x, -224.0);
        assert_eq!(first.center.z, -224.0);
    }

    #[test]
    fn frustum_culls_distant_chunks() {
        let mut chunks = build_chunk_grid(64.0, 32.0, 2);
        // Camera high above looking down with a narrow frustum; only a subset of
        // chunks near the view center should remain visible.
        let view = Mat4::look_at_rh(Vec3::new(0.0, 100.0, 0.0), Vec3::ZERO, Vec3::NEG_Z);
        let proj = Mat4::perspective_rh(30.0f32.to_radians(), 1.0, 0.1, 200.0);
        let frustum = Frustum::from_view_projection(proj * view);
        let visible = cull_and_select_lod(
            &mut chunks,
            Vec3::new(0.0, 100.0, 0.0),
            &frustum,
            -10.0,
            10.0,
            2.0,
        );
        assert!(
            visible.len() < chunks.len(),
            "expected culling to reduce chunk count, got {}/{}",
            visible.len(),
            chunks.len()
        );
    }
}
