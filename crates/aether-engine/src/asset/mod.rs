//! Asset management module.
//!
//! Manages loading, caching, and lifetime of runtime assets.

/// Asynchronous asset loading adapter.
pub mod async_loader;
/// Stable identities for typed assets.
pub mod id;
/// External model file loaders.
pub mod loaders;
/// Material definitions.
pub mod material;
/// Resolved material asset lifecycle and GPU generation cache.
pub mod material_asset;
/// Mesh asset types.
pub mod mesh;
/// Versioned Prefab documents and scene instances.
pub mod prefab;
/// Built-in mesh registry.
pub mod registry;
/// Shader utilities.
pub mod shader;
/// Typed asset lifecycle and worker result queue.
pub mod store;
/// Terrain splatting material.
pub mod terrain_material;
/// Texture asset types.
pub mod texture;
/// GPU texture cache.
pub mod texture_cache;

mod asset_trait;
mod handle;
mod manager;
mod store_types;

pub use asset_trait::Asset;
pub use handle::Handle;
pub use id::{AssetId, AssetIdError, AssetIdParseError, AssetKind, CanonicalPath};
pub use manager::AssetManager;
pub use store::{
    canonicalize, ApplyOutcome, AssetError, AssetError as StoreError, AssetPayload, AssetResult,
    AssetStore, AssetStoreConfig, CancellationToken, FrameBoundary, GpuAssetKey, GpuCommitCache,
    GpuCommitContext, LoadRequest, LoadStateView, LoadTicket, ReloadTicket,
};
