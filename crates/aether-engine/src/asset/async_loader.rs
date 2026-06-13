//! Asynchronous asset loading foundation.
//!
//! Loads raw/decoded asset data on a background thread so the main
//! thread is never blocked by disk I/O. Completed loads are surfaced
//! through `AsyncHandle<T>` handles and processed in `update()`.
//!
//! The current implementation uses a single background worker thread.
//! A thread pool can be swapped in later without changing the public
//! API.

use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

use tracing::{debug, error, trace};

use super::{Asset, Handle};

/// Lifecycle state of an asynchronous asset load.
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
///
/// Clone freely; the underlying asset is reference-counted.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct AsyncHandle<T: 'static> {
    id: u64,
    _marker: std::marker::PhantomData<T>,
}

impl<T: 'static> Copy for AsyncHandle<T> {}

impl<T: 'static> Clone for AsyncHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> AsyncHandle<T> {
    fn new(id: u64) -> Self {
        Self {
            id,
            _marker: std::marker::PhantomData,
        }
    }

    /// Return the underlying ID.
    pub fn id(&self) -> u64 {
        self.id
    }
}

fn typed_loader<T: Asset + 'static + Send + Sync>(path: &Path) -> BoxedAsset {
    T::load(path)
        .map(|asset| Box::new(asset) as Box<dyn Any + Send + Sync>)
        .map_err(|e| e.to_string())
}

/// Central asynchronous asset loader.
///
/// `AsyncAssetLoader` owns a background worker thread and a registry of
/// pending/completed loads. Call `update()` once per frame to process
/// completions.
pub struct AsyncAssetLoader {
    next_id: AtomicU64,
    slots: HashMap<u64, AssetSlot>,
    job_tx: std::sync::mpsc::Sender<LoadJob>,
    result_rx: std::sync::mpsc::Receiver<CompletedLoad>,
}

impl Default for AsyncAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncAssetLoader {
    /// Create a new async asset loader with one background worker.
    pub fn new() -> Self {
        let (job_tx, job_rx) = std::sync::mpsc::channel::<LoadJob>();
        let (result_tx, result_rx) = std::sync::mpsc::channel::<CompletedLoad>();

        thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                trace!("Loading asset in background: {}", job.path.display());
                let result = (job.loader)(&job.path);
                if result_tx
                    .send(CompletedLoad {
                        id: job.id,
                        result,
                        modified: std::fs::metadata(&job.path).and_then(|m| m.modified()).ok(),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            next_id: AtomicU64::new(1),
            slots: HashMap::new(),
            job_tx,
            result_rx,
        }
    }

    /// Request an asset to be loaded asynchronously.
    ///
    /// If the same path is already being tracked, this returns a new
    /// handle but does not deduplicate work.
    pub fn load<T>(&mut self, path: impl AsRef<Path>) -> AsyncHandle<T>
    where
        T: Asset + 'static + Send + Sync,
    {
        let path = path.as_ref().to_path_buf();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        debug!("Queue async load: {} (id: {})", path.display(), id);

        let loader_fn: LoaderFnPtr = typed_loader::<T>;

        self.slots.insert(
            id,
            AssetSlot {
                path: path.clone(),
                state: SlotState::Loading,
                last_modified: None,
                loader: Some(loader_fn),
            },
        );

        let job = LoadJob {
            id,
            path,
            loader: Box::new(loader_fn),
        };
        if self.job_tx.send(job).is_err() {
            if let Some(slot) = self.slots.get_mut(&id) {
                slot.state = SlotState::Failed("background worker disconnected".into());
            }
        }

        AsyncHandle::new(id)
    }

    /// Poll for completed loads and optionally queue hot-reloads.
    ///
    /// Call this once per frame on the main thread.
    pub fn update(&mut self) {
        // Drain completed loads.
        while let Ok(completed) = self.result_rx.try_recv() {
            if let Some(slot) = self.slots.get_mut(&completed.id) {
                match completed.result {
                    Ok(asset) => {
                        trace!(
                            "Asset load complete: {} (id: {})",
                            slot.path.display(),
                            completed.id
                        );
                        slot.state = SlotState::Ready(Arc::from(asset));
                        slot.last_modified = completed.modified;
                    }
                    Err(msg) => {
                        error!(
                            "Asset load failed: {} (id: {}): {}",
                            slot.path.display(),
                            completed.id,
                            msg
                        );
                        slot.state = SlotState::Failed(msg);
                    }
                }
            }
        }

        // Hot-reload: poll file mtimes for ready assets.
        let mut reloads = Vec::new();
        for (id, slot) in &self.slots {
            if let SlotState::Ready(_) = slot.state {
                let current_modified = std::fs::metadata(&slot.path)
                    .and_then(|m| m.modified())
                    .ok();
                if current_modified.is_some() && current_modified != slot.last_modified {
                    trace!("Hot-reload detected: {}", slot.path.display());
                    reloads.push((*id, slot.path.clone()));
                }
            }
        }

        for (id, path) in reloads {
            let loader_fn = match self.slots.get(&id).and_then(|slot| slot.loader) {
                Some(f) => f,
                None => {
                    error!("Missing loader for hot-reload slot {}", id);
                    continue;
                }
            };

            if let Some(slot) = self.slots.get_mut(&id) {
                slot.state = SlotState::Loading;
                slot.last_modified = None;
            }

            let job = LoadJob {
                id,
                path,
                loader: Box::new(loader_fn),
            };
            if self.job_tx.send(job).is_err() {
                if let Some(slot) = self.slots.get_mut(&id) {
                    slot.state = SlotState::Failed("background worker disconnected".into());
                }
            }
        }
    }

    /// Get the current state of an asset load.
    pub fn state<T: 'static + Send + Sync>(&self, handle: AsyncHandle<T>) -> AssetLoadState<T> {
        match self.slots.get(&handle.id) {
            Some(slot) => match &slot.state {
                SlotState::Loading => AssetLoadState::Loading,
                SlotState::Ready(asset) => asset
                    .clone()
                    .downcast::<T>()
                    .map(AssetLoadState::Ready)
                    .unwrap_or_else(|_| AssetLoadState::Failed("type mismatch".into())),
                SlotState::Failed(msg) => AssetLoadState::Failed(msg.clone()),
            },
            None => AssetLoadState::Failed("unknown handle".into()),
        }
    }

    /// Return the path associated with a handle, if any.
    pub fn path<T>(&self, handle: AsyncHandle<T>) -> Option<&Path> {
        self.slots.get(&handle.id).map(|s| s.path.as_path())
    }
}

struct AssetSlot {
    path: PathBuf,
    state: SlotState,
    last_modified: Option<SystemTime>,
    loader: Option<LoaderFnPtr>,
}

enum SlotState {
    Loading,
    Ready(Arc<dyn Any + Send + Sync>),
    Failed(String),
}

/// Type-erased loader result returned by background workers.
pub type BoxedAsset = Result<Box<dyn Any + Send + Sync>, String>;

/// Function pointer that knows how to load a concrete asset type.
type LoaderFnPtr = fn(&Path) -> BoxedAsset;

type LoaderFn = Box<dyn FnOnce(&Path) -> BoxedAsset + Send>;

struct LoadJob {
    id: u64,
    path: PathBuf,
    loader: LoaderFn,
}

struct CompletedLoad {
    id: u64,
    result: Result<Box<dyn Any + Send + Sync>, String>,
    modified: Option<SystemTime>,
}

/// Compatibility: turn an async handle into the synchronous `Handle<T>` once
/// the asset is ready. The returned handle is only meaningful if the asset
/// has completed loading.
pub fn to_sync_handle<T: 'static>(handle: AsyncHandle<T>) -> Handle<T> {
    Handle::new(handle.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    #[derive(Debug, PartialEq)]
    struct TestAsset(String);

    impl Asset for TestAsset {
        fn load(path: &Path) -> anyhow::Result<Self> {
            Ok(TestAsset(fs::read_to_string(path)?))
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aether_async_loader_{}_{}",
            std::process::id(),
            name
        ));
        p
    }

    fn wait_for_ready<T: 'static + Send + Sync>(
        loader: &mut AsyncAssetLoader,
        handle: AsyncHandle<T>,
    ) -> Arc<T> {
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

        let mut loader = AsyncAssetLoader::new();
        let handle = loader.load::<TestAsset>(&path);

        let asset = wait_for_ready(&mut loader, handle);
        assert_eq!(asset.as_ref(), &TestAsset("hello terrain".into()));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn async_load_reports_failure_for_missing_file() {
        let path = temp_path("missing");
        let mut loader = AsyncAssetLoader::new();
        let handle = loader.load::<TestAsset>(&path);

        for _ in 0..200 {
            loader.update();
            if let AssetLoadState::Failed(_) = loader.state(handle) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(
            matches!(loader.state(handle), AssetLoadState::Failed(_)),
            "expected failed state, got {:?}",
            loader.state(handle)
        );
    }

    #[test]
    fn async_load_state_is_loading_before_update() {
        let path = temp_path("loading_state");
        fs::write(&path, b"x").unwrap();

        let mut loader = AsyncAssetLoader::new();
        let handle = loader.load::<TestAsset>(&path);

        assert_eq!(loader.state(handle), AssetLoadState::Loading);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn async_load_multiple_assets_complete() {
        let path_a = temp_path("multi_a");
        let path_b = temp_path("multi_b");
        fs::write(&path_a, b"asset_a").unwrap();
        fs::write(&path_b, b"asset_b").unwrap();

        let mut loader = AsyncAssetLoader::new();
        let handle_a = loader.load::<TestAsset>(&path_a);
        let handle_b = loader.load::<TestAsset>(&path_b);

        let asset_a = wait_for_ready(&mut loader, handle_a);
        let asset_b = wait_for_ready(&mut loader, handle_b);

        assert_eq!(asset_a.as_ref(), &TestAsset("asset_a".into()));
        assert_eq!(asset_b.as_ref(), &TestAsset("asset_b".into()));

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn async_load_hot_reload_detects_mtime_change() {
        let path = temp_path("hot_reload");
        fs::write(&path, b"v1").unwrap();

        let mut loader = AsyncAssetLoader::new();
        let handle = loader.load::<TestAsset>(&path);
        let _ = wait_for_ready(&mut loader, handle);

        // Ensure the next write has a different mtime.
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, b"v2").unwrap();

        // update() currently logs the limitation; it still transitions the
        // slot back to Loading when a change is detected.
        loader.update();
        assert_eq!(loader.state(handle), AssetLoadState::Loading);

        fs::remove_file(&path).ok();
    }
}
