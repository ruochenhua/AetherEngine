//! Scene management module.
//!
//! Data types for describing 3D scenes declaratively. Scenes are serialized
//! as RON files and loaded by the Launcher.

/// Scene configuration types (camera, lights, objects, terrain, etc.).
pub mod config;
/// Scene loader.
pub mod loader;
/// Opaque material schema resolution.
pub mod material;
/// Scene serializer.
pub mod serializer;

pub use config::{
    AtmosphereConfig, CameraConfig, CloudConfig, GodRayConfig, LightConfig, MaterialConfig,
    MeshRef, ObjectConfig, PhysicsBodyConfig, PhysicsColliderConfig, PhysicsColliderShapeConfig,
    PhysicsConfig, SceneDescription, TerrainConfig, TerrainGeometry, TerrainLayerConfig,
    TerrainSource, TransformConfig, TransparentMaterialConfig, WaterConfig,
};

pub use crate::asset::prefab::{
    ComponentPatch, PrefabAsset, PrefabComponentKind, PrefabDocument, PrefabError,
    PrefabInstanceConfig, PrefabNode, PrefabOverrides, RemovedComponent,
};
