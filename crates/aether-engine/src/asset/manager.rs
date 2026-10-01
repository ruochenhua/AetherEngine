use super::{
    Asset, AssetError, AssetId, AssetResult, AssetStore, AssetStoreConfig, FrameBoundary, Handle,
    ReloadTicket,
};
use std::path::Path;

/// Compatibility adapter backed by the typed asset store.
pub struct AssetManager {
    store: AssetStore,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    /// Create an asset manager rooted at the current working directory.
    pub fn new() -> Self {
        Self {
            store: AssetStore::new(AssetStoreConfig::default()),
        }
    }

    /// Create an asset adapter rooted at a project directory.
    pub fn with_project_root(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: AssetStore::new(AssetStoreConfig::new(root)),
        }
    }

    /// Load an asset from a file path, returning an existing handle on cache hit.
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Handle<T>> {
        self.store
            .load_legacy_sync(path.as_ref())
            .map_err(anyhow::Error::from)
    }

    /// Get an asset by handle.
    pub fn get<T: Asset>(&self, handle: Handle<T>) -> Option<std::sync::Arc<T>> {
        self.store.get(handle).ok()
    }

    /// Return the stable identity associated with a ready handle.
    pub fn asset_id<T: Asset>(&self, handle: Handle<T>) -> Result<AssetId, AssetError> {
        self.store.asset_id_for_handle(handle)
    }

    /// Return the project root used to resolve project-relative assets.
    pub fn project_root(&self) -> &Path {
        self.store.project_root()
    }

    /// Queue a versioned reload for a currently loaded asset.
    pub fn reload<T: Asset>(&mut self, handle: Handle<T>) -> Result<ReloadTicket, AssetError> {
        self.store.reload(handle)
    }

    /// Drain completed asynchronous asset work without blocking.
    pub fn poll_results(&mut self, max: usize) -> Result<Vec<AssetResult>, AssetError> {
        self.store.poll_results(max)
    }

    /// Resolve dependencies on a material result and apply it at a frame boundary.
    pub fn apply_material_result(
        &mut self,
        result: AssetResult,
        boundary: FrameBoundary,
    ) -> Result<super::ApplyOutcome, AssetError> {
        let root = self.store.project_root().to_path_buf();
        let resolved = super::material_asset::resolve_material_result(result, &root, self);
        self.store.apply_result(resolved, boundary)
    }

    /// Stop and join the backing asset worker before application shutdown.
    pub fn shutdown(&mut self) -> Result<(), AssetError> {
        self.store.shutdown()
    }

    /// Check if an asset is loaded.
    pub fn is_loaded<T>(&self, handle: Handle<T>) -> bool {
        self.store.contains_handle(handle)
    }

    #[cfg(test)]
    pub(crate) fn load_from<T: Asset>(
        &mut self,
        path: &str,
        asset: T,
    ) -> anyhow::Result<Handle<T>> {
        self.store
            .insert_ready(Path::new(path), asset)
            .map_err(anyhow::Error::from)
    }
}
