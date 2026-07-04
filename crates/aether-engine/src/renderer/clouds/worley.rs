//! 3D Cellular / Worley noise for cloud shapes.
//!
//! Uses a 3D grid of feature points with random jitter.
//! At each voxel, computes distance to the 2 nearest feature points
//! and returns F2 - F1 (edge-like cellular pattern) mapped to [0, 1].

use glam::{IVec3, Vec3};

/// Generate a 3D R8Unorm Worley noise texture.
/// `size` must be a power of 2 for clean wrapping.
pub fn worley_noise_3d(size: u32) -> Vec<u8> {
    let cell_count = 8u32; // 8x8x8 feature-point grid
    let cell_size = size as f32 / cell_count as f32;
    let mut data = vec![0u8; (size * size * size) as usize];

    // Pre-generate feature points: cell_count³ points, each jittered within its cell
    let mut features = Vec::with_capacity((cell_count * cell_count * cell_count) as usize);
    for z in 0..cell_count {
        for y in 0..cell_count {
            for x in 0..cell_count {
                let seed = IVec3::new(x as i32, y as i32, z as i32);
                let jitter = hash3_jitter(seed);
                features.push(Vec3::new(
                    (x as f32 + jitter.x) * cell_size,
                    (y as f32 + jitter.y) * cell_size,
                    (z as f32 + jitter.z) * cell_size,
                ));
            }
        }
    }

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32);

                // Find 2 nearest feature points (with wrapping)
                let mut d0 = f32::MAX;
                let mut d1 = f32::MAX;

                // Search 3x3x3 neighbor cells for wrapping support
                for dz in -1i32..=1i32 {
                    for dy in -1i32..=1i32 {
                        for dx in -1i32..=1i32 {
                            let cx = ((x as f32 / cell_size) as i32 + dx)
                                .rem_euclid(cell_count as i32) as u32;
                            let cy = ((y as f32 / cell_size) as i32 + dy)
                                .rem_euclid(cell_count as i32) as u32;
                            let cz = ((z as f32 / cell_size) as i32 + dz)
                                .rem_euclid(cell_count as i32) as u32;

                            let idx = (cz * cell_count * cell_count + cy * cell_count + cx) as usize;
                            let fp = features[idx];

                            // Toroidal wrapping distance
                            let mut diff = p - fp;
                            diff.x = wrap_diff(diff.x, size as f32);
                            diff.y = wrap_diff(diff.y, size as f32);
                            diff.z = wrap_diff(diff.z, size as f32);
                            let d = diff.length();

                            if d < d0 {
                                d1 = d0;
                                d0 = d;
                            } else if d < d1 {
                                d1 = d;
                            }
                        }
                    }
                }

                // Inverted F1 distance creates billowy blob shapes suitable for cumulus clouds.
                let value = 1.0 - (d0 / cell_size);
                let normalized = value.clamp(0.0, 1.0);
                let idx = (z * size * size + y * size + x) as usize;
                data[idx] = (normalized * 255.0) as u8;
            }
        }
    }

    data
}

/// Deterministic jitter in [0.0, 1.0) per cell.
fn hash3_jitter(cell: IVec3) -> Vec3 {
    let h = |n: i32| -> f32 {
        let mut h = (n.wrapping_mul(374761393) ^ (n >> 13)).wrapping_mul(1274126177);
        h = h ^ (h >> 16);
        h as u32 as f32 / u32::MAX as f32
    };
    Vec3::new(h(cell.x), h(cell.y), h(cell.z))
}

/// Wrap a signed delta for toroidal distance.
fn wrap_diff(d: f32, size: f32) -> f32 {
    if d > size * 0.5 { d - size } else if d < -size * 0.5 { d + size } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worley_output_has_correct_size() {
        let data = worley_noise_3d(32);
        assert_eq!(data.len(), 32 * 32 * 32);
    }

    #[test]
    fn worley_output_has_dynamic_range() {
        let data = worley_noise_3d(16);
        let min = *data.iter().min().unwrap();
        let max = *data.iter().max().unwrap();
        assert!(min < max, "expected non-empty dynamic range");
    }

    #[test]
    fn worley_is_deterministic() {
        let a = worley_noise_3d(16);
        let b = worley_noise_3d(16);
        assert_eq!(a, b);
    }
}
