//! Cloud configuration.

use serde::{Deserialize, Serialize};

/// Volumetric cloud quality preset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudQuality {
    /// Low quality — reduced noise resolution and step counts.
    Low,
    /// Medium quality — balanced quality and performance.
    Medium,
    /// High quality — increased noise resolution and step counts.
    High,
}

impl Default for CloudQuality {
    fn default() -> Self {
        Self::Medium
    }
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
