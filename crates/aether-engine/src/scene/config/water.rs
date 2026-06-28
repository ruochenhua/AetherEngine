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
    /// Optional dudv map path for refraction UV distortion.
    #[serde(default)]
    pub dudv_map: Option<String>,
    /// Optional normal map path for surface normal perturbation.
    #[serde(default)]
    pub normal_map: Option<String>,
    /// UV scale for the dudv/normal map tiling.
    #[serde(default = "default_water_texture_scale")]
    pub texture_scale: f32,
    /// Distortion strength contributed by the dudv map.
    #[serde(default = "default_water_dudv_strength")]
    pub dudv_strength: f32,
    /// Strength of the normal-map perturbation (0 = geometry normal only).
    #[serde(default = "default_water_normal_strength")]
    pub normal_strength: f32,
    /// Scale applied to water thickness when computing depth-based color absorption.
    #[serde(default = "default_water_depth_scale")]
    pub depth_scale: f32,
    /// Base UV flow speed for the first animated texture layer [u, v].
    #[serde(default = "default_water_flow_speed")]
    pub flow_speed: [f32; 2],
    /// UV flow speed for the second animated texture layer [u, v].
    #[serde(default = "default_water_flow_speed_2")]
    pub flow_speed_2: [f32; 2],
    /// UV scale multiplier for the second animated texture layer.
    #[serde(default = "default_water_secondary_scale")]
    pub secondary_scale: f32,
    /// Exponent for the Blinn-Phong sun specular highlight.
    #[serde(default = "default_water_specular_power")]
    pub specular_power: f32,
    /// Enable planar reflection rendering for this water surface.
    #[serde(default = "default_water_reflection_enabled")]
    pub reflection_enabled: bool,
    /// Resolution scale for the planar reflection texture relative to the screen.
    #[serde(default = "default_water_reflection_resolution_scale")]
    pub reflection_resolution_scale: f32,
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
            dudv_map: None,
            normal_map: None,
            texture_scale: default_water_texture_scale(),
            dudv_strength: default_water_dudv_strength(),
            normal_strength: default_water_normal_strength(),
            depth_scale: default_water_depth_scale(),
            flow_speed: default_water_flow_speed(),
            flow_speed_2: default_water_flow_speed_2(),
            secondary_scale: default_water_secondary_scale(),
            specular_power: default_water_specular_power(),
            reflection_enabled: default_water_reflection_enabled(),
            reflection_resolution_scale: default_water_reflection_resolution_scale(),
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

fn default_water_texture_scale() -> f32 {
    4.0
}

fn default_water_dudv_strength() -> f32 {
    0.02
}

fn default_water_normal_strength() -> f32 {
    1.0
}

fn default_water_depth_scale() -> f32 {
    0.15
}

fn default_water_flow_speed() -> [f32; 2] {
    [0.03, 0.01]
}

fn default_water_flow_speed_2() -> [f32; 2] {
    [-0.02, 0.015]
}

fn default_water_secondary_scale() -> f32 {
    0.7
}

fn default_water_specular_power() -> f32 {
    128.0
}

fn default_water_reflection_enabled() -> bool {
    false
}

fn default_water_reflection_resolution_scale() -> f32 {
    0.5
}
