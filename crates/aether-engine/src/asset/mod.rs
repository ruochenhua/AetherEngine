//! Asset management module.
//!
//! Manages loading, caching, and lifetime of runtime assets.

pub mod mesh;
pub mod texture;
pub mod material;
pub mod shader;

use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, trace};

/// Unique handle to an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    id: u64,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Handle<T> {
    /// Create a new handle from an ID.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the underlying ID.
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// Asset manager.
///
/// Central registry for all loaded assets. Provides deduplication
/// and reference counting.
pub struct AssetManager {
    next_id: u64,
    assets: HashMap<u64, Arc<dyn Any + Send + Sync>>,
    path_to_id: HashMap<PathBuf, u64>,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    /// Create a new asset manager.
    pub fn new() -> Self {
        Self {
            next_id: 1,
            assets: HashMap::new(),
            path_to_id: HashMap::new(),
        }
    }

    /// Load an asset from a file path.
    ///
    /// If the asset has already been loaded, returns the existing handle.
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Handle<T>>
    where
        T: 'static + Send + Sync,
    {
        let path = path.as_ref().to_path_buf();

        if let Some(&id) = self.path_to_id.get(&path) {
            trace!("Asset cache hit: {} (id: {})", path.display(), id);
            return Ok(Handle::new(id));
        }

        debug!("Loading asset: {}", path.display());
        let asset = T::load(&path)?;
        let id = self.next_id;
        self.next_id += 1;

        self.assets.insert(id, Arc::new(asset));
        self.path_to_id.insert(path, id);

        Ok(Handle::new(id))
    }

    /// Get an asset by handle.
    pub fn get<T: 'static + Send + Sync>(&self, handle: Handle<T>) -> Option<Arc<T>> {
        self.assets
            .get(&handle.id())
            .and_then(|a| a.clone().downcast::<T>().ok())
    }

    /// Check if an asset is loaded.
    pub fn is_loaded(&self, handle: Handle<impl Any>) -> bool {
        self.assets.contains_key(&handle.id())
    }
}

/// Trait for loadable assets.
pub trait Asset: Sized + Send + Sync {
    /// Load the asset from a file path.
    fn load(path: &Path) -> anyhow::Result<Self>;
}
