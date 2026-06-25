//! God ray configuration.

use serde::{Deserialize, Serialize};

/// God ray (volumetric light shafts) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GodRayConfig {
    /// Number of ray-marching samples.
    #[serde(default = "default_godray_samples")]
    pub samples: u32,
    /// Density falloff along the ray.
    #[serde(default = "default_godray_density")]
    pub density: f32,
    /// Decay factor per sample.
    #[serde(default = "default_godray_decay")]
    pub decay: f32,
    /// Intensity weight.
    #[serde(default = "default_godray_weight")]
    pub weight: f32,
    /// Final exposure multiplier.
    #[serde(default = "default_godray_exposure")]
    pub exposure: f32,
}

impl Default for GodRayConfig {
    fn default() -> Self {
        Self {
            samples: default_godray_samples(),
            density: default_godray_density(),
            decay: default_godray_decay(),
            weight: default_godray_weight(),
            exposure: default_godray_exposure(),
        }
    }
}

fn default_godray_samples() -> u32 {
    64
}

fn default_godray_density() -> f32 {
    0.5
}

fn default_godray_decay() -> f32 {
    0.95
}

fn default_godray_weight() -> f32 {
    0.5
}

fn default_godray_exposure() -> f32 {
    0.3
}
