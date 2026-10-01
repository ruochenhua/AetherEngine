use super::{Asset, AssetError, AssetId, AssetStore, AssetStoreConfig, Handle};
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
