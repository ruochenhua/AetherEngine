//! Pseudo-curl noise for volumetric cloud displacement.
//!
//! An ad-hoc 2D warp field derived from finite differences of a scalar
//! potential. This is not a true 3D curl; it produces a 2D displacement
//! vector perpendicular to the local gradient of the potential field.
//! Output is RG8Snorm: a 2D warp vector per voxel.

use super::value_noise::fbm_perlin_3d;
use glam::Vec3;

/// Generate a 3D pseudo-curl warp field. Each voxel gets a 2D warp vector
/// derived from finite differences of a scalar potential, stored as signed bytes.
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
            assert!(i32::from(x) <= 127);
            assert!(i32::from(y) <= 127);
        }
    }

    #[test]
    fn curl_is_deterministic() {
        let a = curl_noise_3d(16);
        let b = curl_noise_3d(16);
        assert_eq!(a, b);
    }
}
