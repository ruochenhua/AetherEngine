//! Volumetric cloud configuration.
//!
//! One-to-one port of the parameters exposed by NadirRoGue/RenderEngine's
//! VolumetricCloudProgram + WorldConfig. The cloud layer is modelled as a
//! spherical shell around `planet_radius`, with inner/outer radii computed as
//! `planet_radius + bottom_altitude` and `planet_radius + top_altitude`.

use serde::{Deserialize, Serialize};

/// Volumetric cloud quality preset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CloudQuality {
    /// Low quality — smallest noise textures (32³ Perlin-Worley, 16³ Worley,
    /// 512² weather).
    Low,
    /// Medium quality — balanced quality and performance (64³ Perlin-Worley,
    /// 32³ Worley, 1024² weather).
    #[default]
    Medium,
    /// High quality — largest noise textures (128³ Perlin-Worley, 32³ Worley,
    /// 2048² weather).
    High,
}

/// Volumetric cloud configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudConfig {
    /// Radius of the planet used to compute the spherical cloud shell.
    #[serde(default = "default_cloud_planet_radius")]
    pub planet_radius: f32,
    /// Bottom altitude of the cloud layer above the planet surface.
    #[serde(default = "default_cloud_bottom_altitude")]
    pub bottom_altitude: f32,
    /// Top altitude of the cloud layer above the planet surface.
    #[serde(default = "default_cloud_top_altitude")]
    pub top_altitude: f32,
    /// Coverage multiplier in [0, 1] (RenderEngine's `coverageMultiplier`).
    #[serde(default = "default_cloud_coverage")]
    pub coverage: f32,
    /// Wind direction on the XZ plane [x, z].
    #[serde(default = "default_cloud_wind_direction")]
    pub wind_direction: [f32; 2],
    /// Wind speed in world units per second.
    #[serde(default = "default_cloud_wind_speed")]
    pub wind_speed: f32,
    /// Quality preset controlling procedural noise texture resolution.
    #[serde(default)]
    pub quality: CloudQuality,
    /// Weather texture UV scale (RenderEngine's `weatherScale`).
    #[serde(default = "default_cloud_weather_scale")]
    pub weather_scale: f32,
    /// Base shape noise UV scale (RenderEngine's `baseNoiseScale`).
    #[serde(default = "default_cloud_base_noise_scale")]
    pub base_noise_scale: f32,
    /// High-frequency erosion noise scale (RenderEngine's `highFreqNoiseScale`).
    #[serde(default = "default_cloud_high_freq_noise_scale")]
    pub high_freq_noise_scale: f32,
    /// High-frequency erosion horizontal UV scale (RenderEngine's `highFreqNoiseUVScale`).
    #[serde(default = "default_cloud_high_freq_uv_scale")]
    pub high_freq_uv_scale: f32,
    /// High-frequency erosion vertical scale (RenderEngine's `highFreqNoiseHScale`).
    #[serde(default = "default_cloud_high_freq_h_scale")]
    pub high_freq_h_scale: f32,
    /// Vertical shear applied to cloud tops as wind moves clouds.
    #[serde(default = "default_cloud_cloud_top_offset")]
    pub cloud_top_offset: f32,
    /// Cloud type selector (0=stratus, 0.5=stratocumulus, 1=cumulus).
    #[serde(default = "default_cloud_cloud_type")]
    pub cloud_type: f32,
    /// Maximum distance at which clouds are still ray-marched.
    #[serde(default = "default_cloud_max_render_dist")]
    pub max_render_dist: f32,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            planet_radius: default_cloud_planet_radius(),
            bottom_altitude: default_cloud_bottom_altitude(),
            top_altitude: default_cloud_top_altitude(),
            coverage: default_cloud_coverage(),
            wind_direction: default_cloud_wind_direction(),
            wind_speed: default_cloud_wind_speed(),
            quality: CloudQuality::default(),
            weather_scale: default_cloud_weather_scale(),
            base_noise_scale: default_cloud_base_noise_scale(),
            high_freq_noise_scale: default_cloud_high_freq_noise_scale(),
            high_freq_uv_scale: default_cloud_high_freq_uv_scale(),
            high_freq_h_scale: default_cloud_high_freq_h_scale(),
            cloud_top_offset: default_cloud_cloud_top_offset(),
            cloud_type: default_cloud_cloud_type(),
            max_render_dist: default_cloud_max_render_dist(),
        }
    }
}

fn default_cloud_planet_radius() -> f32 {
    6360.0
}

fn default_cloud_bottom_altitude() -> f32 {
    80.0
}

fn default_cloud_top_altitude() -> f32 {
    120.0
}

fn default_cloud_coverage() -> f32 {
    0.5
}

fn default_cloud_wind_direction() -> [f32; 2] {
    [1.0, 0.0]
}

fn default_cloud_wind_speed() -> f32 {
    0.5
}

fn default_cloud_weather_scale() -> f32 {
    1.0
}

fn default_cloud_base_noise_scale() -> f32 {
    1.0
}

fn default_cloud_high_freq_noise_scale() -> f32 {
    1.0
}

fn default_cloud_high_freq_uv_scale() -> f32 {
    150.0
}

fn default_cloud_high_freq_h_scale() -> f32 {
    4.0
}

fn default_cloud_cloud_top_offset() -> f32 {
    0.0
}

fn default_cloud_cloud_type() -> f32 {
    0.5
}

fn default_cloud_max_render_dist() -> f32 {
    30000.0
}
