//! Cloud configuration.

use serde::{Deserialize, Serialize};

/// Volumetric cloud quality preset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CloudQuality {
    /// Low quality — reduced noise resolution and step counts.
    Low,
    /// Medium quality — balanced quality and performance.
    #[default]
    Medium,
    /// High quality — increased noise resolution and step counts.
    High,
}

/// Volumetric cloud configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudConfig {
    /// Bottom altitude of the cloud slab (world-space Y).
    #[serde(default = "default_cloud_bottom_altitude")]
    pub bottom_altitude: f32,
    /// Top altitude of the cloud slab (world-space Y).
    #[serde(default = "default_cloud_top_altitude")]
    pub top_altitude: f32,
    /// Cloud coverage threshold in [0, 1].
    #[serde(default = "default_cloud_coverage")]
    pub coverage: f32,
    /// Overall density multiplier.
    #[serde(default = "default_cloud_density")]
    pub density: f32,
    /// Wind direction on the XZ plane [x, z].
    #[serde(default = "default_cloud_wind_direction")]
    pub wind_direction: [f32; 2],
    /// Wind speed in world units per second.
    #[serde(default = "default_cloud_wind_speed")]
    pub wind_speed: f32,
    /// Quality preset controlling noise resolution and step counts.
    #[serde(default)]
    pub quality: CloudQuality,
    /// Large-scale weather map frequency (world-space scale).
    #[serde(default = "default_cloud_weather_scale")]
    pub weather_scale: f32,
    /// Base shape noise frequency (world-space scale).
    #[serde(default = "default_cloud_base_noise_scale")]
    pub base_noise_scale: f32,
    /// High-frequency erosion noise frequency (world-space scale).
    #[serde(default = "default_cloud_high_freq_noise_scale")]
    pub high_freq_noise_scale: f32,
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
            bottom_altitude: default_cloud_bottom_altitude(),
            top_altitude: default_cloud_top_altitude(),
            coverage: default_cloud_coverage(),
            density: default_cloud_density(),
            wind_direction: default_cloud_wind_direction(),
            wind_speed: default_cloud_wind_speed(),
            quality: CloudQuality::default(),
            weather_scale: default_cloud_weather_scale(),
            base_noise_scale: default_cloud_base_noise_scale(),
            high_freq_noise_scale: default_cloud_high_freq_noise_scale(),
            cloud_top_offset: default_cloud_cloud_top_offset(),
            cloud_type: default_cloud_cloud_type(),
            max_render_dist: default_cloud_max_render_dist(),
        }
    }
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

fn default_cloud_density() -> f32 {
    1.0
}

fn default_cloud_wind_direction() -> [f32; 2] {
    [1.0, 0.0]
}

fn default_cloud_wind_speed() -> f32 {
    2.0
}

fn default_cloud_weather_scale() -> f32 {
    0.00008
}

fn default_cloud_base_noise_scale() -> f32 {
    0.0003
}

fn default_cloud_high_freq_noise_scale() -> f32 {
    0.003
}

fn default_cloud_cloud_top_offset() -> f32 {
    500.0
}

fn default_cloud_cloud_type() -> f32 {
    0.75
}

fn default_cloud_max_render_dist() -> f32 {
    30000.0
}
