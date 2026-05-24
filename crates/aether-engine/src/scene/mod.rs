//! Scene management module.
//!
//! Handles scene loading, serialization, and entity instantiation.

/// Scene loading utilities.
pub mod loader;
/// Scene serialization formats.
pub mod serializer;

use crate::math::*;
use serde::{Deserialize, Serialize};

/// A serializable scene description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Scene name.
    pub name: String,
    /// Scene entities.
    pub entities: Vec<SceneEntity>,
}

/// A serializable entity description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEntity {
    /// Entity name.
    pub name: String,
    /// Transform component.
    pub transform: Option<TransformData>,
    /// Mesh component.
    pub mesh: Option<MeshData>,
    /// Light component.
    pub light: Option<LightData>,
    /// Camera component.
    pub camera: Option<CameraData>,
}

/// Serializable transform data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformData {
    /// Translation vector [x, y, z].
    pub translation: [f32; 3],
    /// Rotation quaternion [x, y, z, w].
    pub rotation: [f32; 4],
    /// Scale vector [x, y, z].
    pub scale: [f32; 3],
}

impl Default for TransformData {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// Serializable mesh data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshData {
    /// Path to the mesh file.
    pub mesh_path: String,
    /// Optional path to the material file.
    pub material_path: Option<String>,
}

/// Serializable light data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightData {
    /// Light type: "directional", "point", or "spot".
    pub light_type: String,
    /// Light color [r, g, b].
    pub color: [f32; 3],
    /// Light intensity multiplier.
    pub intensity: f32,
    /// Whether this light casts shadows.
    pub cast_shadow: bool,
}

/// Serializable camera data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraData {
    /// Vertical field of view in degrees.
    pub fov: f32,
    /// Near clip plane distance.
    pub near: f32,
    /// Far clip plane distance.
    pub far: f32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            name: "Untitled".to_string(),
            entities: Vec::new(),
        }
    }
}
