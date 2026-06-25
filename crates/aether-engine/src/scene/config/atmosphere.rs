//! Atmosphere configuration.

use serde::{Deserialize, Serialize};

/// Physical atmosphere configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtmosphereConfig {
    /// Direction toward the sun [x, y, z]. Defaults to a low sun angle.
    #[serde(default = "default_sun_direction")]
    pub sun_direction: [f32; 3],
    /// Planet radius in world units. The camera is assumed to sit on the surface.
    #[serde(default = "default_planet_radius")]
    pub planet_radius: f32,
    /// Atmosphere shell thickness above the planet surface.
    #[serde(default = "default_atmosphere_height")]
    pub atmosphere_height: f32,
    /// Rayleigh scattering coefficients (RGB).
    #[serde(default = "default_rayleigh_scattering")]
    pub rayleigh_scattering: [f32; 3],
    /// Rayleigh density scale height.
    #[serde(default = "default_rayleigh_scale_height")]
    pub rayleigh_scale_height: f32,
    /// Mie scattering coefficients (RGB).
    #[serde(default = "default_mie_scattering")]
    pub mie_scattering: [f32; 3],
    /// Mie density scale height.
    #[serde(default = "default_mie_scale_height")]
    pub mie_scale_height: f32,
    /// Mie asymmetry parameter (g) in [-1, 1].
    #[serde(default = "default_mie_asymmetry")]
    pub mie_asymmetry: f32,
    /// Sun intensity multiplier.
    #[serde(default = "default_sun_intensity")]
    pub sun_intensity: f32,
}

impl Default for AtmosphereConfig {
    fn default() -> Self {
        Self {
            sun_direction: default_sun_direction(),
            planet_radius: default_planet_radius(),
            atmosphere_height: default_atmosphere_height(),
            rayleigh_scattering: default_rayleigh_scattering(),
            rayleigh_scale_height: default_rayleigh_scale_height(),
            mie_scattering: default_mie_scattering(),
            mie_scale_height: default_mie_scale_height(),
            mie_asymmetry: default_mie_asymmetry(),
            sun_intensity: default_sun_intensity(),
        }
    }
}

fn default_sun_direction() -> [f32; 3] {
    [0.0, 0.2, -1.0]
}

fn default_planet_radius() -> f32 {
    6360.0
}

fn default_atmosphere_height() -> f32 {
    100.0
}

fn default_rayleigh_scattering() -> [f32; 3] {
    [0.005802, 0.013558, 0.033100]
}

fn default_rayleigh_scale_height() -> f32 {
    8.0
}

fn default_mie_scattering() -> [f32; 3] {
    [0.004000, 0.004000, 0.004000]
}

fn default_mie_scale_height() -> f32 {
    1.2
}

fn default_mie_asymmetry() -> f32 {
    0.758
}

fn default_sun_intensity() -> f32 {
    20.0
}
