//! Compatibility asset manager and handle types retained for existing consumers.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tracing::debug;

use super::store::{
    AssetError, AssetId, AssetStore, AssetStoreConfig, FrameBoundary, LoadStateView, TypedAsset,
};

/// Unique handle to an asset.
#[derive(Debug)]
pub struct Handle<T> {
    id: u64,
    generation: u32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self::with_generation(self.id, self.generation)
    }
}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.generation == other.generation
    }
}
impl<T> Eq for Handle<T> {}
impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.generation.hash(state);
    }
}

impl<T> Handle<T> {
    /// Create a new handle from an ID.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            generation: 1,
            _phantom: std::marker::PhantomData,
        }
    }

    pub(crate) fn with_generation(id: u64, generation: u32) -> Self {
        Self {
            id,
            generation,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the underlying ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Return the generation captured by this handle.
    pub fn generation(&self) -> u32 {
        self.generation
    }
}

/// Synchronous compatibility adapter over the typed asset store.
///
/// `load` preserves the old blocking interface while decoding and caching through
/// the store's canonical identity and worker lifecycle.
pub struct AssetManager {
    pub(super) store: AssetStore,
    project_root: PathBuf,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    /// Create a manager rooted at the current project directory.
    pub fn new() -> Self {
        Self::with_root(".")
    }

    /// Create a compatibility manager rooted at a project asset directory.
    pub fn with_root(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref();
        let project_root = if project_root.is_absolute() {
            project_root.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(project_root)
        };
        Self {
            store: AssetStore::new(AssetStoreConfig::new(super::async_loader::filesystem_root())),
            project_root,
        }
    }

    /// Load an asset from a file path.
    ///
    /// If the asset has already been loaded, returns the existing handle.
    pub fn load<T>(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Handle<T>>
    where
        T: TypedAsset + 'static + Send + Sync,
    {
        let path = path.as_ref();
        debug!("Loading asset: {}", path.display());
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.project_root.join(path)
        };
        let path = absolute_path
            .strip_prefix(super::async_loader::filesystem_root())
            .map_err(|_| super::store::AssetError::PathEscape)?;
        let (handle, _) = self.store.request::<T>(path)?;
        loop {
            match self.store.state(handle.clone())? {
                LoadStateView::Ready { .. } => return Ok(handle),
                LoadStateView::Failed { error, .. } => return Err(error.into()),
                LoadStateView::Loading { .. } => {}
            }

            let results = self.store.poll_results(16)?;
            if results.is_empty() {
                thread::sleep(Duration::from_millis(1));
            }
            for result in results {
                if let Ok(ticket) = self
                    .store
                    .apply_result(result, FrameBoundary { frame_id: 1 })
                {
                    let _ = self.store.commit_result(
                        &ticket,
                        FrameBoundary { frame_id: 1 },
                        |_, _, _| Ok(()),
                    );
                }
            }
        }
    }

    /// Get an asset by handle.
    pub fn get<T: TypedAsset>(&self, handle: Handle<T>) -> Option<std::sync::Arc<T>> {
        self.store.get(handle).ok()
    }

    /// Return the persistent identity for a loaded handle.
    pub fn asset_id<T>(&self, handle: Handle<T>) -> Result<AssetId, AssetError> {
        self.store.asset_id(handle)
    }

    /// Check if an asset is loaded.
    pub fn is_loaded<T: TypedAsset>(&self, handle: Handle<T>) -> bool {
        matches!(self.store.state(handle), Ok(LoadStateView::Ready { .. }))
    }
}

/// Trait for loadable assets.
pub trait Asset: Sized + Send + Sync {
    /// Load the asset from a file path.
    fn load(path: &Path) -> anyhow::Result<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::store::{AssetKind, TypedAsset};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    static LOADS: AtomicUsize = AtomicUsize::new(0);
    static LOAD_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Debug)]
    struct TestAsset(String);

    impl Asset for TestAsset {
        fn load(path: &Path) -> anyhow::Result<Self> {
            LOADS.fetch_add(1, Ordering::SeqCst);
            Ok(Self(fs::read_to_string(path)?))
        }
    }

    impl TypedAsset for TestAsset {
        const KIND: AssetKind = AssetKind::CpuMesh;
    }

    #[test]
    fn synchronous_manager_adapter_deduplicates_canonical_project_paths() {
        let _guard = LOAD_LOCK.lock().unwrap();
        LOADS.store(0, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("aether_legacy_store_{}", std::process::id()));
        fs::create_dir_all(root.join("alias")).unwrap();
        fs::write(root.join("asset.txt"), "asset payload").unwrap();
        let mut manager = AssetManager::with_root(root.clone());

        let first = manager.load::<TestAsset>("alias/../asset.txt").unwrap();
        let second = manager.load::<TestAsset>("asset.txt").unwrap();

        assert_eq!(first, second);
        assert_eq!(manager.get(first).unwrap().0, "asset payload");
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn synchronous_manager_adapter_accepts_absolute_paths_outside_project_root() {
        let _guard = LOAD_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("aether_legacy_absolute_{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("asset.txt");
        fs::write(&path, "external asset").unwrap();
        let mut manager = AssetManager::new();

        let handle = manager.load::<TestAsset>(&path).unwrap();

        assert_eq!(manager.get(handle).unwrap().0, "external asset");
        fs::remove_dir_all(root).unwrap();
    }
}
