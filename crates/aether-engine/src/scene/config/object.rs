//! Object, transform, and material configuration.

pub use super::material::MaterialConfig;
use serde::{Deserialize, Serialize};

/// Mesh reference — either a built-in shape or an external file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshRef {
    /// Built-in mesh identified by name ("cube", "sphere", "quad").
    Builtin(String),
    /// External mesh file path.
    File(String),
}

/// Object entity configuration kept separate from the mesh/transform types.
pub mod object_config;
pub use object_config::ObjectConfig;

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
