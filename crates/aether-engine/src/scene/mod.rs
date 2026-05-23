//! Scene management module.
//!
//! Handles scene loading, serialization, and entity instantiation.

pub mod loader;
pub mod serializer;

use crate::ecs::{Component, Entity, World};
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
    pub translation: [f32; 3],
    pub rotation: [f32; 4],    // Quaternion
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
    pub mesh_path: String,
    pub material_path: Option<String>,
}

/// Serializable light data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightData {
    pub light_type: String,     // "directional", "point", "spot"
    pub color: [f32; 3],
    pub intensity: f32,
    pub cast_shadow: bool,
}

/// Serializable camera data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraData {
    pub fov: f32,
    pub near: f32,
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
