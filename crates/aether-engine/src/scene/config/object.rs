//! Object, transform, and material configuration.

use serde::{Deserialize, Serialize};

/// Mesh reference — either a built-in shape or an external file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshRef {
    /// Built-in mesh identified by name ("cube", "sphere", "quad").
    Builtin(String),
    /// External mesh file path.
    File(String),
}

/// Object (renderable entity) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectConfig {
    /// Human-readable name for debugging.
    #[serde(default)]
    pub name: String,
    /// Mesh reference.
    pub mesh: MeshRef,
    /// Transform.
    #[serde(default)]
    pub transform: TransformConfig,
    /// PBR material parameters.
    #[serde(default)]
    pub material: MaterialConfig,
}

/// Transform data for an object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformConfig {
    /// Translation [x, y, z].
    #[serde(default)]
    pub translation: [f32; 3],
    /// Rotation quaternion [x, y, z, w].
    #[serde(default = "default_rotation")]
    pub rotation: [f32; 4],
    /// Scale [x, y, z].
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: default_rotation(),
            scale: default_scale(),
        }
    }
}

/// PBR material parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    #[serde(default)]
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    #[serde(default)]
    pub metallic: f32,
}

fn default_albedo() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}

impl Default for MaterialConfig {
    fn default() -> Self {
        Self {
            albedo: default_albedo(),
            roughness: 0.5,
            metallic: 0.0,
        }
    }
}
