//! Asset management modules and compatibility exports.

/// Asynchronous asset loading.
pub mod async_loader;
/// External model file loaders.
pub mod loaders;
/// Material definitions.
pub mod material;
/// Mesh asset types.
pub mod mesh;
/// Built-in mesh registry.
pub mod registry;
/// Shader utilities.
pub mod shader;
/// Typed asset store and lifecycle.
pub mod store;
/// Terrain splatting material.
pub mod terrain_material;
/// Texture asset types.
pub mod texture;
/// GPU texture cache.
pub mod texture_cache;

mod legacy;
pub use legacy::{Asset, AssetManager, Handle};

#[cfg(test)]
mod store_tests;
