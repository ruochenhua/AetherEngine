//! 2D Weather map — cloud coverage distribution.
//!
//! A low-resolution coverage mask that determines where clouds should
//! form (high values) vs clear sky (low values).

use super::value_noise::fbm_perlin_2d;
use glam::Vec2;

/// Generate a 2D R8Unorm weather map with natural-looking coverage patterns.
pub fn generate_weather_map_2d(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size) as usize];

    // Perlin-like FBM creates organic coverage regions
    for y in 0..size {
        for x in 0..size {
            let pos = Vec2::new(x as f32 / size as f32, y as f32 / size as f32);
            let value = fbm_perlin_2d(pos * 4.0, 3, 2.0, 0.5);
            let idx = (y * size + x) as usize;
            data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    data
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
    fn weather_map_values_have_dynamic_range() {
        let data = generate_weather_map_2d(16);
        let min = *data.iter().min().unwrap();
        let max = *data.iter().max().unwrap();
        assert!(min < max, "expected non-empty dynamic range");
    }
}
