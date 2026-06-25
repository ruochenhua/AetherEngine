//! Scene management module.
//!
//! Data types for describing 3D scenes declaratively. Scenes are serialized
//! as RON files and loaded by the Launcher.

/// Scene configuration types (camera, lights, objects, terrain, etc.).
pub mod config;
/// Scene loader.
pub mod loader;
/// Scene serializer.
pub mod serializer;

pub use config::{
    AtmosphereConfig, CameraConfig, CloudConfig, GodRayConfig, LightConfig, MaterialConfig,
    MeshRef, ObjectConfig, SceneDescription, TerrainConfig, TerrainGeometry, TerrainLayerConfig,
    TerrainSource, TransformConfig, WaterConfig,
};
