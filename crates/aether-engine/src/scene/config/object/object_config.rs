use super::{MaterialConfig, TransformConfig};
use serde::{Deserialize, Serialize};

/// Object (renderable entity) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectConfig {
    /// Human-readable name for debugging.
    #[serde(default)]
    pub name: String,
    /// Mesh reference.
    pub mesh: super::MeshRef,
    /// Transform.
    #[serde(default)]
    pub transform: TransformConfig,
    /// PBR material parameters.
    #[serde(default)]
    pub material: MaterialConfig,
    /// Whether the object is rendered.
    #[serde(default = "default_visibility")]
    pub visible: bool,
}

fn default_visibility() -> bool {
    true
}
