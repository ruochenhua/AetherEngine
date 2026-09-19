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
    /// Light position [x, y, z] (for Point and Spot lights).
    #[serde(default)]
    pub position: [f32; 3],
    /// Light color [r, g, b].
    #[serde(default = "default_light_color")]
    pub color: [f32; 3],
    /// Light intensity.
    #[serde(default = "default_intensity")]
    pub intensity: f32,
    /// Maximum influence distance for local lights.
    #[serde(default = "default_light_range")]
    pub range: f32,
    /// Inner cone angle in radians for Spot lights.
    #[serde(default = "default_inner_cone_angle")]
    pub inner_cone_angle: f32,
    /// Outer cone angle in radians for Spot lights.
    #[serde(default = "default_outer_cone_angle")]
    pub outer_cone_angle: f32,
}

fn default_light_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_intensity() -> f32 {
    1.0
}
fn default_light_range() -> f32 {
    10.0
}
fn default_inner_cone_angle() -> f32 {
    0.35
}
fn default_outer_cone_angle() -> f32 {
    0.7
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            direction: [0.0, -1.0, 0.0],
            position: [0.0; 3],
            color: default_light_color(),
            intensity: default_intensity(),
            range: default_light_range(),
            inner_cone_angle: default_inner_cone_angle(),
            outer_cone_angle: default_outer_cone_angle(),
        }
    }
}

impl LightConfig {
    /// Validate values before spawning them into a render world.
    pub fn validate(&self, index: usize) -> anyhow::Result<()> {
        let path = format!("lights[{index}]");
        if !self
            .color
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
        {
            anyhow::bail!("{path}.color must contain finite non-negative values");
        }
        if !self.intensity.is_finite() || self.intensity < 0.0 {
            anyhow::bail!("{path}.intensity must be finite and non-negative");
        }
        if !self.position.iter().all(|value| value.is_finite()) {
            anyhow::bail!("{path}.position must contain finite values");
        }
        match self.light_type {
            LightType::Directional => {
                if !self.direction.iter().all(|value| value.is_finite())
                    || glam::Vec3::from_array(self.direction).length_squared() == 0.0
                {
                    anyhow::bail!("{path}.direction must be a non-zero finite vector");
                }
            }
            LightType::Point | LightType::Spot => {
                if !self.range.is_finite() || self.range <= 0.0 {
                    anyhow::bail!("{path}.range must be finite and greater than zero");
                }
            }
        }
        if self.light_type == LightType::Spot
            && (!self.inner_cone_angle.is_finite()
                || !self.outer_cone_angle.is_finite()
                || self.inner_cone_angle < 0.0
                || self.inner_cone_angle > self.outer_cone_angle
                || self.outer_cone_angle > std::f32::consts::PI)
        {
            anyhow::bail!(
                "{path}.inner_cone_angle/outer_cone_angle must satisfy 0 <= inner <= outer <= PI"
            );
        }
        Ok(())
    }
}
