//! Curl noise for volumetric cloud displacement.
//!
//! Curl of a 3D Perlin-like potential field.
//! Output is RG8Snorm: 2D curl vector (perpendicular to gradient) per voxel.

use glam::Vec3;

/// Generate a 3D curl noise field. Each voxel gets a 2D curl vector
/// (x-component of curl, y-component of curl) stored as signed bytes.
///
/// The shader uses these to warp the sampling position of the density
/// textures, creating wispy, swirling cloud edges.
pub fn curl_noise_3d(size: u32) -> Vec<[i8; 2]> {
    let mut data = Vec::with_capacity((size * size * size) as usize);

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let scale = 0.05; // large-scale curl features
                let pos = Vec3::new(x as f32, y as f32, z as f32) * scale;

                // Finite-difference curl of potential field
                let eps = 0.01;
                let pot = |p: Vec3| -> f32 { potential(p) };

                let dp_dy = (pot(pos + Vec3::Y * eps) - pot(pos - Vec3::Y * eps)) / (2.0 * eps);
                let dp_dz = (pot(pos + Vec3::Z * eps) - pot(pos - Vec3::Z * eps)) / (2.0 * eps);
                let dp_dx = (pot(pos + Vec3::X * eps) - pot(pos - Vec3::X * eps)) / (2.0 * eps);

                // curl = (dp/dy - dp/dz, dp/dz - dp/dx, dp/dx - dp/dy) -> keep x, y
                let curl_x = dp_dy - dp_dz;
                let curl_y = dp_dz - dp_dx;

                // Map [-1, 1] to [-127, 127]
                let ix = (curl_x.clamp(-1.0, 1.0) * 127.0) as i8;
                let iy = (curl_y.clamp(-1.0, 1.0) * 127.0) as i8;
                data.push([ix, iy]);
            }
        }
    }

    data
}

fn potential(p: Vec3) -> f32 {
    fbm_perlin_3d(p, 3, 2.0, 0.5)
}

fn fbm_perlin_3d(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
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

    let c00 = c000 + (c100 - c000) * u;
    let c01 = c001 + (c101 - c001) * u;
    let c10 = c010 + (c110 - c010) * u;
    let c11 = c011 + (c111 - c011) * u;

    let c0 = c00 + (c10 - c00) * v;
    let c1 = c01 + (c11 - c01) * v;

    c0 + (c1 - c0) * w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_output_has_correct_size() {
        let data = curl_noise_3d(16);
        assert_eq!(data.len(), (16 * 16 * 16) as usize);
    }

    #[test]
    fn curl_values_in_i8_range() {
        let data = curl_noise_3d(16);
        for &[x, y] in &data {
            assert!(x >= -127);
            assert!(y >= -127);
        }
    }

    #[test]
    fn curl_is_deterministic() {
        let a = curl_noise_3d(16);
        let b = curl_noise_3d(16);
        assert_eq!(a, b);
    }
}
