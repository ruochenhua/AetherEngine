//! Light configuration.

use crate::renderer::light::LightType;
use serde::{Deserialize, Serialize};

/// Light configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightConfig {
    /// Type of light.
    pub light_type: LightType,
    /// Light direction [x, y, z] (for Directional lights).
    #[serde(default)]
    pub direction: [f32; 3],
    /// Light color [r, g, b].
    #[serde(default = "default_light_color")]
    pub color: [f32; 3],
    /// Light intensity.
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

fn default_light_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_intensity() -> f32 {
    1.0
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            direction: [0.0, -1.0, 0.0],
            color: default_light_color(),
            intensity: default_intensity(),
        }
    }
}
