//! Compatibility adapter for the typed [`AssetStore`](super::AssetStore).
//!
//! New code should use `AssetStore` directly. This adapter keeps the previous
//! `AsyncHandle` and `AssetLoadState` surface while sharing one worker, typed
//! state table, cancellation system, and result queue.

use super::{
    Asset, AssetError, AssetId, AssetStore, AssetStoreConfig, FrameBoundary, Handle, LoadStateView,
    LoadTicket,
};
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

mod reload;

/// Lifecycle state of an asynchronously loaded asset.
#[derive(Debug, Clone, PartialEq)]
pub enum AssetLoadState<T> {
    /// Asset is still being loaded.
    Loading,
    /// Asset finished loading successfully.
    Ready(Arc<T>),
    /// Asset failed to load.
    Failed(String),
}

/// Handle to an asynchronously loaded asset.
pub struct AsyncHandle<T: 'static> {
    id: u64,
    sync_handle: Option<Handle<T>>,
}

impl<T: 'static> std::fmt::Debug for AsyncHandle<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("AsyncHandle")
            .field(&self.id)
            .finish()
    }
}

impl<T: 'static> PartialEq for AsyncHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for AsyncHandle<T> {}

impl<T: 'static> std::hash::Hash for AsyncHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.id, state);
    }
}

impl<T: 'static> Copy for AsyncHandle<T> {}

impl<T: 'static> Clone for AsyncHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> AsyncHandle<T> {
    fn new(id: u64, sync_handle: Option<Handle<T>>) -> Self {
        Self { id, sync_handle }
    }

    /// Return the stable adapter-local identifier.
    pub fn id(&self) -> u64 {
        self.id
    }
}

struct AsyncRecord<T: Asset> {
    handle: Option<Handle<T>>,
    ticket: Option<LoadTicket>,
    asset_id: Option<AssetId>,
    path: PathBuf,
    last_modified: Option<SystemTime>,
    error: Option<String>,
}

struct ErasedRecord {
    value: Box<dyn Any + Send + Sync>,
    refresh: fn(&AssetStore, &mut (dyn Any + Send + Sync)),
    reload_changed: fn(&mut AssetStore, &mut (dyn Any + Send + Sync)),
}

/// Central compatibility adapter for asynchronous asset requests.
pub struct AsyncAssetLoader {
    store: AssetStore,
    slots: HashMap<u64, ErasedRecord>,
    next_id: u64,
    frame_id: u64,
}

impl Default for AsyncAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncAssetLoader {
    /// Create a loader rooted at the current working directory.
    pub fn new() -> Self {
        Self::with_config(AssetStoreConfig::default())
    }

    /// Create a loader rooted at a project directory.
    pub fn with_project_root(root: impl Into<PathBuf>) -> Self {
        Self::with_config(AssetStoreConfig::new(root))
    }

    /// Create a loader with explicit worker shutdown settings.
    pub fn with_config(config: AssetStoreConfig) -> Self {
        Self {
            store: AssetStore::new(config),
            slots: HashMap::new(),
            next_id: 1,
            frame_id: 0,
        }
    }

    /// Request an asset to be loaded asynchronously.
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> AsyncHandle<T> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let path = path.as_ref().to_path_buf();
        let normalized = self.store.compatibility_path(&path);
        let requested = normalized
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|relative| {
                let asset_id = self.store.asset_id::<T>(relative)?;
                let (handle, ticket) = self.store.request::<T>(relative)?;
                Ok((relative.clone(), asset_id, handle, ticket))
            });
        let (relative_path, asset_id, handle, ticket, error) = match requested {
            Ok((relative, asset_id, handle, ticket)) => {
                (relative, Some(asset_id), Some(handle), Some(ticket), None)
            }
            Err(error) => (path.clone(), None, None, None, Some(error.to_string())),
        };
        let last_modified = std::fs::metadata(self.store.project_root().join(&relative_path))
            .and_then(|metadata| metadata.modified())
            .ok();
        let record = AsyncRecord {
            handle,
            ticket,
            asset_id,
            path: relative_path,
            last_modified,
            error,
        };
        let sync_handle = record.handle;
        self.slots.insert(
            id,
            ErasedRecord {
                value: Box::new(record),
                refresh: refresh_record::<T>,
                reload_changed: reload::reload_changed::<T>,
            },
        );
        AsyncHandle::new(id, sync_handle)
    }

    /// Process completed results and queue file-change reloads.
    pub fn update(&mut self) {
        self.frame_id = self.frame_id.saturating_add(1);
        if let Ok(results) = self.store.poll_results(128) {
            for result in results {
                let _ = self
                    .store
                    .apply_result(result, FrameBoundary::new(self.frame_id));
            }
        }
        for slot in self.slots.values_mut() {
            (slot.refresh)(&self.store, slot.value.as_mut());
        }
        let store = &mut self.store;
        for slot in self.slots.values_mut() {
            (slot.reload_changed)(store, slot.value.as_mut());
        }
    }

    /// Get the current state of an asset load.
    pub fn state<T: Asset>(&self, handle: AsyncHandle<T>) -> AssetLoadState<T> {
        let Some(record) = self.slots.get(&handle.id) else {
            return AssetLoadState::Failed("unknown handle".into());
        };
        let Some(record) = record.value.downcast_ref::<AsyncRecord<T>>() else {
            return AssetLoadState::Failed("asset handle type mismatch".into());
        };
        if let Some(error) = &record.error {
            return AssetLoadState::Failed(error.clone());
        }
        let Some(asset_handle) = record.handle else {
            return AssetLoadState::Failed("unknown handle".into());
        };
        match self.store.state(asset_handle) {
            Ok(LoadStateView::Loading { .. }) => AssetLoadState::Loading,
            Ok(LoadStateView::Ready { .. }) => match self.store.get(asset_handle) {
                Ok(asset) => AssetLoadState::Ready(asset),
                Err(error) => AssetLoadState::Failed(error.to_string()),
            },
            Ok(LoadStateView::Failed { error, .. }) => AssetLoadState::Failed(error.to_string()),
            Err(error) => AssetLoadState::Failed(error.to_string()),
        }
    }

    /// Cancel the current load request for an adapter handle.
    pub fn cancel<T: Asset>(&mut self, handle: AsyncHandle<T>) -> Result<(), AssetError> {
        let record = self
            .slots
            .get(&handle.id)
            .and_then(|slot| slot.value.downcast_ref::<AsyncRecord<T>>())
            .ok_or(AssetError::StaleHandle)?;
        let ticket = record.ticket.ok_or(AssetError::StaleHandle)?;
        self.store.cancel(&ticket)
    }

    /// Return the path associated with a handle, if any.
    pub fn path<T: Asset>(&self, handle: AsyncHandle<T>) -> Option<&Path> {
        self.slots
            .get(&handle.id)?
            .value
            .downcast_ref::<AsyncRecord<T>>()
            .map(|record| record.path.as_path())
    }

    /// Stop and join the shared typed-store worker.
    pub fn shutdown(&mut self) -> Result<(), AssetError> {
        self.store.shutdown()
    }
}

/// Convert an async handle into its initial synchronous handle.
///
/// This is intended for use after the first load has completed. After a reload,
/// obtain the current generation from `AssetStore::current_handle` instead.
pub fn to_sync_handle<T: 'static>(handle: AsyncHandle<T>) -> Handle<T> {
    handle.sync_handle.unwrap_or_else(|| Handle::new(handle.id))
}

fn refresh_record<T: Asset>(store: &AssetStore, value: &mut (dyn Any + Send + Sync)) {
    let Some(record) = value.downcast_mut::<AsyncRecord<T>>() else {
        return;
    };
    let Some(asset_id) = &record.asset_id else {
        return;
    };
    if let Ok(handle) = store.current_handle::<T>(asset_id) {
        record.handle = Some(handle);
    }
}

/// Type-erased legacy loader result retained for source compatibility.
pub type BoxedAsset = Result<Box<dyn Any + Send + Sync>, String>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::thread;
    use std::time::Duration;

    #[derive(Debug, PartialEq)]
    struct TestAsset(String);

    impl Asset for TestAsset {
        const KIND: super::super::AssetKind = super::super::AssetKind::CpuMesh;

        fn load(path: &Path) -> anyhow::Result<Self> {
            Ok(TestAsset(fs::read_to_string(path)?))
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "aether_async_loader_{}_{}",
            std::process::id(),
            name
        ));
        path
    }

    fn loader_for(path: &Path) -> AsyncAssetLoader {
        AsyncAssetLoader::with_project_root(path.parent().unwrap())
    }

    fn wait_for_ready<T: Asset>(loader: &mut AsyncAssetLoader, handle: AsyncHandle<T>) -> Arc<T> {
        for _ in 0..200 {
            loader.update();
            if let AssetLoadState::Ready(asset) = loader.state(handle) {
                return asset;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("asset did not become ready in time");
    }

    #[test]
    fn async_load_completes_with_asset_content() {
        let path = temp_path("completes");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"hello terrain").unwrap();
        let mut loader = loader_for(&path);
        let handle = loader.load::<TestAsset>(&path);
        assert_eq!(
            wait_for_ready(&mut loader, handle).as_ref(),
            &TestAsset("hello terrain".into())
        );
        loader.shutdown().unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn async_load_reports_failure_for_missing_file() {
        let path = temp_path("missing");
        let mut loader = loader_for(&path);
        let handle = loader.load::<TestAsset>(&path);
        for _ in 0..200 {
            loader.update();
            if matches!(loader.state(handle), AssetLoadState::Failed(_)) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(loader.state(handle), AssetLoadState::Failed(_)));
        loader.shutdown().unwrap();
    }

    #[test]
    fn async_load_state_is_loading_before_update() {
        let path = temp_path("loading_state");
        fs::write(&path, b"x").unwrap();
        let mut loader = loader_for(&path);
        let handle = loader.load::<TestAsset>(&path);
        assert_eq!(loader.state(handle), AssetLoadState::Loading);
        loader.shutdown().unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn async_load_multiple_assets_complete() {
        let path_a = temp_path("multi_a");
        let path_b = temp_path("multi_b");
        fs::write(&path_a, b"asset_a").unwrap();
        fs::write(&path_b, b"asset_b").unwrap();
        let mut loader = loader_for(&path_a);
        let handle_a = loader.load::<TestAsset>(&path_a);
        let handle_b = loader.load::<TestAsset>(&path_b);
        assert_eq!(
            wait_for_ready(&mut loader, handle_a).as_ref(),
            &TestAsset("asset_a".into())
        );
        assert_eq!(
            wait_for_ready(&mut loader, handle_b).as_ref(),
            &TestAsset("asset_b".into())
        );
        loader.shutdown().unwrap();
        fs::remove_file(path_a).ok();
        fs::remove_file(path_b).ok();
    }

    #[test]
    fn async_load_hot_reload_detects_mtime_change() {
        let path = temp_path("hot_reload");
        fs::write(&path, b"v1").unwrap();
        let mut loader = loader_for(&path);
        let handle = loader.load::<TestAsset>(&path);
        let _ = wait_for_ready(&mut loader, handle);
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, b"v2").unwrap();
        loader.update();
        assert_eq!(loader.state(handle), AssetLoadState::Loading);
        loader.shutdown().unwrap();
        fs::remove_file(path).ok();
    }
}
