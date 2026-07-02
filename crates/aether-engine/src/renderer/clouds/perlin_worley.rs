//! Perlin-Worley noise blend — re-maps base Worley with high-frequency Perlin detail.
//!
//! Output = Worley * detail_weight + Perlin * (1 - detail_weight)
//! where detail_weight = 0.7, creating eroded cloud edges with internal structure.

use super::value_noise::fbm_perlin_3d;
use super::worley::worley_noise_3d;
use glam::Vec3;

/// Generate a 3D R8Unorm Perlin-Worley blend noise texture from an existing
/// Worley noise buffer. The `worley_data` slice must have length `size³`.
pub fn perlin_worley_from_worley(worley_data: &[u8], size: u32) -> Vec<u8> {
    let detail_weight: f32 = 0.7;
    let mut data = vec![0u8; (size * size * size) as usize];

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32) / size as f32;

                // Worley from the provided buffer as base structure.
                let base = worley_data[(z * size * size + y * size + x) as usize] as f32 / 255.0;
                // High-freq Perlin for detail (3 octaves)
                let detail = fbm_perlin_3d(p * 16.0, 3, 2.0, 0.5).clamp(0.0, 1.0);

                let value = base * detail_weight + detail * (1.0 - detail_weight);
                let idx = (z * size * size + y * size + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    data
}

/// Generate a 3D R8Unorm Perlin-Worley blend noise texture.
pub fn perlin_worley_noise_3d(size: u32) -> Vec<u8> {
    let worley = worley_noise_3d(size);
    perlin_worley_from_worley(&worley, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_worley_output_has_correct_size() {
        let data = perlin_worley_noise_3d(32);
        assert_eq!(data.len(), 32 * 32 * 32);
    }

    #[test]
    fn perlin_worley_output_has_dynamic_range() {
        let data = perlin_worley_noise_3d(16);
        let min = *data.iter().min().unwrap();
        let max = *data.iter().max().unwrap();
        assert!(min < max, "expected non-empty dynamic range");
    }

    #[test]
    fn perlin_worley_is_deterministic() {
        let a = perlin_worley_noise_3d(16);
        let b = perlin_worley_noise_3d(16);
        assert_eq!(a, b);
    }
}
