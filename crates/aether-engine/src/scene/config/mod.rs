//! Scene configuration types.
//!
//! This module contains the data structures that describe individual elements
//! of a scene, such as cameras, lights, objects, terrain, atmosphere, water,
//! clouds, and god rays.

pub mod atmosphere;
pub mod camera;
pub mod clouds;
pub mod god_ray;
pub mod light;
pub mod object;
pub mod terrain;
pub mod water;

pub use atmosphere::*;
pub use camera::*;
pub use clouds::*;
pub use god_ray::*;
pub use light::*;
pub use object::*;
pub use terrain::*;
pub use water::*;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Scene description — the root of a RON scene file
// ---------------------------------------------------------------------------

/// Top-level scene description, deserialized from a `.ron` file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneDescription {
    /// Human-readable scene name.
    pub name: String,
    /// Camera initial configuration.
    pub camera: CameraConfig,
    /// Lights in the scene.
    #[serde(default)]
    pub lights: Vec<LightConfig>,
    /// Ambient light intensity (0.0 – 1.0).
    #[serde(default)]
    pub ambient: f32,
    /// Objects in the scene.
    #[serde(default)]
    pub objects: Vec<ObjectConfig>,
    /// Optional global terrain configuration.
    #[serde(default)]
    pub terrain: Option<TerrainConfig>,
    /// Optional physical atmosphere configuration.
    #[serde(default)]
    pub atmosphere: Option<AtmosphereConfig>,
    /// Optional water surface configuration.
    #[serde(default)]
    pub water: Option<WaterConfig>,
    /// Optional volumetric cloud configuration.
    #[serde(default)]
    pub clouds: Option<CloudConfig>,
    /// Optional god ray (volumetric light) configuration.
    #[serde(default)]
    pub god_ray: Option<GodRayConfig>,
}

// ---------------------------------------------------------------------------
// RON parsing
// ---------------------------------------------------------------------------

impl SceneDescription {
    /// Parse a scene from a RON string.
    pub fn from_ron(content: &str) -> anyhow::Result<Self> {
        let desc: SceneDescription = ron::de::from_str(content)?;
        Ok(desc)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
