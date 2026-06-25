//! Water configuration.

use serde::{Deserialize, Serialize};

/// Water surface configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterConfig {
    /// Water level (world-space Y).
    #[serde(default = "default_water_level")]
    pub level: f32,
    /// Wave travel direction on the XZ plane [x, z].
    #[serde(default = "default_water_wave_direction")]
    pub wave_direction: [f32; 2],
    /// Wave amplitude.
    #[serde(default = "default_water_wave_amplitude")]
    pub wave_amplitude: f32,
    /// Wave wavelength.
    #[serde(default = "default_water_wave_wavelength")]
    pub wave_wavelength: f32,
    /// Wave speed.
    #[serde(default = "default_water_wave_speed")]
    pub wave_speed: f32,
    /// Wave steepness (0 = sine, 1 = sharp crests).
    #[serde(default = "default_water_wave_steepness")]
    pub wave_steepness: f32,
    /// Shallow water color (RGB).
    #[serde(default = "default_water_color")]
    pub water_color: [f32; 3],
    /// Deep water color (RGB).
    #[serde(default = "default_water_deep_color")]
    pub deep_color: [f32; 3],
    /// Fresnel power.
    #[serde(default = "default_water_fresnel_power")]
    pub fresnel_power: f32,
    /// Refraction UV distortion scale.
    #[serde(default = "default_water_refraction_scale")]
    pub refraction_scale: f32,
    /// Reflection intensity multiplier.
    #[serde(default = "default_water_reflectivity")]
    pub reflectivity: f32,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            level: default_water_level(),
            wave_direction: default_water_wave_direction(),
            wave_amplitude: default_water_wave_amplitude(),
            wave_wavelength: default_water_wave_wavelength(),
            wave_speed: default_water_wave_speed(),
            wave_steepness: default_water_wave_steepness(),
            water_color: default_water_color(),
            deep_color: default_water_deep_color(),
            fresnel_power: default_water_fresnel_power(),
            refraction_scale: default_water_refraction_scale(),
            reflectivity: default_water_reflectivity(),
        }
    }
}

fn default_water_level() -> f32 {
    0.0
}

fn default_water_wave_direction() -> [f32; 2] {
    [1.0, 0.5]
}

fn default_water_wave_amplitude() -> f32 {
    0.3
}

fn default_water_wave_wavelength() -> f32 {
    8.0
}

fn default_water_wave_speed() -> f32 {
    2.0
}

fn default_water_wave_steepness() -> f32 {
    0.6
}

fn default_water_color() -> [f32; 3] {
    [0.0, 0.35, 0.45]
}

fn default_water_deep_color() -> [f32; 3] {
    [0.0, 0.15, 0.25]
}

fn default_water_fresnel_power() -> f32 {
    3.0
}

fn default_water_refraction_scale() -> f32 {
    0.02
}

fn default_water_reflectivity() -> f32 {
    0.6
}
