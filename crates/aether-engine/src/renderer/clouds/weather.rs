//! 2D Weather map — cloud coverage distribution.
//!
//! A low-resolution coverage mask that determines where clouds should
//! form (high values) vs clear sky (low values).

use glam::Vec2;

/// Generate a 2D R8Unorm weather map with natural-looking coverage patterns.
pub fn generate_weather_map_2d(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size) as usize];

    // Perlin-like FBM creates organic coverage regions
    for y in 0..size {
        for x in 0..size {
            let pos = Vec2::new(x as f32 / size as f32, y as f32 / size as f32);
            let value = fbm_2d(pos * 4.0, 3, 2.0, 0.5);
            let idx = (y * size + x) as usize;
            data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    data
}

fn fbm_2d(p: Vec2, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_2d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

fn value_noise_2d(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;

    let u = f.x * f.x * (3.0 - 2.0 * f.x);
    let v = f.y * f.y * (3.0 - 2.0 * f.y);

    let hash = |x: i32, y: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        n as f32 / u32::MAX as f32
    };

    let n00 = hash(ix, iy);
    let n10 = hash(ix + 1, iy);
    let n01 = hash(ix, iy + 1);
    let n11 = hash(ix + 1, iy + 1);

    let nx0 = n00 + (n10 - n00) * u;
    let nx1 = n01 + (n11 - n01) * u;

    nx0 + (nx1 - nx0) * v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_map_has_correct_size() {
        let data = generate_weather_map_2d(32);
        assert_eq!(data.len(), 32 * 32);
    }

    #[test]
    fn weather_map_values_in_range() {
        let data = generate_weather_map_2d(16);
        for &v in &data {
            assert!(v <= 255u8);
        }
    }
}
