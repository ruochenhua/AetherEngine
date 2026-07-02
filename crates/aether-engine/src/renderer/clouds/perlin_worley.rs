//! Perlin-Worley noise blend — re-maps base Worley with high-frequency Perlin detail.
//!
//! Output = Worley * detail_weight + Perlin * (1 - detail_weight)
//! where detail_weight = 0.7, creating eroded cloud edges with internal structure.

use glam::Vec3;

/// Generate a 3D R8Unorm Perlin-Worley blend noise texture.
pub fn perlin_worley_noise_3d(size: u32) -> Vec<u8> {
    let detail_weight: f32 = 0.7;
    let mut data = vec![0u8; (size * size * size) as usize];

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32) / size as f32;

                // Low-freq Perlin for base structure (2 octaves)
                let perlin_low = fbm_perlin(p * 4.0, 2, 2.0, 0.5);
                // High-freq Perlin for detail (3 octaves)
                let perlin_high = fbm_perlin(p * 16.0, 3, 2.0, 0.5);

                // Remap: subtract coverage threshold, amplify detail
                let base = perlin_low.clamp(0.0, 1.0);
                let detail = perlin_high.clamp(0.0, 1.0);

                let value = base * detail_weight + detail * (1.0 - detail_weight);
                let idx = (z * size * size + y * size + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    data
}

/// Simple FBM (Fractal Brownian Motion) using 3D Perlin-like value noise.
fn fbm_perlin(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_3d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

/// Trilinearly interpolated value noise (same algorithm as clouds/noise.rs).
fn value_noise_3d(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;

    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let u = f.x * f.x * (3.0 - 2.0 * f.x);
    let v = f.y * f.y * (3.0 - 2.0 * f.y);
    let w = f.z * f.z * (3.0 - 2.0 * f.z);

    let hash = |x: i32, y: i32, z: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ z.wrapping_mul(2086444801);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        n as f32 / u32::MAX as f32
    };

    let c000 = hash(ix, iy, iz);
    let c100 = hash(ix + 1, iy, iz);
    let c010 = hash(ix, iy + 1, iz);
    let c110 = hash(ix + 1, iy + 1, iz);
    let c001 = hash(ix, iy, iz + 1);
    let c101 = hash(ix + 1, iy, iz + 1);
    let c011 = hash(ix, iy + 1, iz + 1);
    let c111 = hash(ix + 1, iy + 1, iz + 1);

    let c00 = c000 * (1.0 - u) + c100 * u;
    let c01 = c001 * (1.0 - u) + c101 * u;
    let c10 = c010 * (1.0 - u) + c110 * u;
    let c11 = c011 * (1.0 - u) + c111 * u;

    let c0 = c00 * (1.0 - v) + c10 * v;
    let c1 = c01 * (1.0 - v) + c11 * v;

    c0 * (1.0 - w) + c1 * w
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
    fn perlin_worley_output_in_u8_range() {
        let data = perlin_worley_noise_3d(16);
        for &v in &data {
            assert!(v <= 255u8);
        }
    }

    #[test]
    fn perlin_worley_is_deterministic() {
        let a = perlin_worley_noise_3d(16);
        let b = perlin_worley_noise_3d(16);
        assert_eq!(a, b);
    }
}
