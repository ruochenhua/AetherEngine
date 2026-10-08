//! Compatibility adapter exposing the original asynchronous loading interface.
//!
//! Requests, worker ownership, result application, reload sequencing, and shutdown
//! are all handled by [`AssetStore`]. This adapter preserves `AsyncHandle` and
//! `update()` for older callers.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use super::store::{
    AssetError, AssetStore, AssetStoreConfig, FrameBoundary, LoadStateView, ReloadTicket,
    TypedAsset,
};
use super::Handle;

/// Lifecycle state of an asynchronous asset load.
#[derive(Debug, Clone, PartialEq)]
pub enum AssetLoadState<T> {
    /// Asset is still being loaded.
    Loading,
    /// Asset finished loading successfully.
    Ready(std::sync::Arc<T>),
    /// Asset failed to load.
    Failed(String),
}

/// Handle to an asynchronously loaded asset.
///
/// Clone freely; the underlying asset is reference-counted. The id is the slot
/// allocated by the shared typed store.
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

    /// Return the underlying store slot id.
    pub fn id(&self) -> u64 {
        self.id
    }
}

type ReloadFn = fn(&mut AssetStore, u64, u32) -> Result<ReloadTicket, AssetError>;

struct AssetSlot {
    path: PathBuf,
    generation: u32,
    last_modified: Option<SystemTime>,
    reload: ReloadFn,
    immediate_error: Option<String>,
}

/// Legacy asynchronous interface backed by the typed [`AssetStore`].
pub struct AsyncAssetLoader {
    project_root: PathBuf,
    store: AssetStore,
    slots: HashMap<u64, AssetSlot>,
    next_failed_id: u64,
    frame_id: u64,
}

impl Default for AsyncAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncAssetLoader {
    /// Create a loader that preserves the legacy current-working-directory behavior.
    ///
    /// Use [`with_root`](Self::with_root) to keep identities relative to a project.
    pub fn new() -> Self {
        Self::with_root(filesystem_root())
    }

    /// Create an adapter rooted at a project asset directory.
    pub fn with_root(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref().to_path_buf();
        Self {
            store: AssetStore::new(AssetStoreConfig::new(project_root.clone())),
            project_root,
            slots: HashMap::new(),
            next_failed_id: u64::MAX,
            frame_id: 1,
        }
    }

    /// Request an asset to be loaded asynchronously.
    ///
    /// Repeated requests with the same typed identity share the store slot and
    /// in-flight decode. Assets must declare a stable [`AssetKind`](super::store::AssetKind).
    pub fn load<T: TypedAsset + 'static + Send + Sync>(
        &mut self,
        path: impl AsRef<Path>,
    ) -> AsyncHandle<T> {
        let path = path.as_ref().to_path_buf();
        let relative_path = match self.relative_path(&path) {
            Ok(path) => path,
            Err(error) => return self.failed_handle::<T>(path, error.to_string()),
        };
        let (handle, _) = match self.store.request::<T>(&relative_path) {
            Ok(request) => request,
            Err(error) => return self.failed_handle::<T>(path, error.to_string()),
        };

        self.slots.entry(handle.id()).or_insert_with(|| AssetSlot {
            path,
            generation: handle.generation(),
            last_modified: None,
            reload: reload_typed::<T>,
            immediate_error: None,
        });
        AsyncHandle::new(handle.id())
    }

    /// Poll results, apply them at the main-thread boundary, and detect file changes.
    pub fn update(&mut self) {
        if let Ok(results) = self.store.poll_results(128) {
            for result in results {
                let slot_id = result.slot_id();
                let applied = self.store.apply_result(
                    result,
                    FrameBoundary {
                        frame_id: self.frame_id,
                    },
                );
                if let Ok(ticket) = applied {
                    if self
                        .store
                        .commit_result(
                            &ticket,
                            FrameBoundary {
                                frame_id: self.frame_id,
                            },
                            |_, _, _| Ok(()),
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        slot.generation = ticket.to_generation;
                        slot.last_modified = modified_time(&slot.path);
                    }
                }
            }
        }
        self.frame_id = self.frame_id.saturating_add(1);

        let reloads: Vec<_> = self
            .slots
            .iter()
            .filter_map(|(id, slot)| {
                if slot.immediate_error.is_some() || slot.last_modified.is_none() {
                    return None;
                }
                let current = modified_time(&slot.path);
                if current.is_some() && current != slot.last_modified {
                    Some((*id, slot.generation, slot.reload))
                } else {
                    None
                }
            })
            .collect();
        for (id, generation, reload) in reloads {
            match reload(&mut self.store, id, generation) {
                Ok(_) => {
                    if let Some(slot) = self.slots.get_mut(&id) {
                        slot.last_modified = None;
                    }
                }
                Err(error) => tracing::warn!("Asset reload failed for slot {id}: {error}"),
            }
        }
    }

    /// Get the current state of an asset load.
    pub fn state<T: TypedAsset + 'static + Send + Sync>(
        &self,
        handle: AsyncHandle<T>,
    ) -> AssetLoadState<T> {
        let Some(slot) = self.slots.get(&handle.id) else {
            return AssetLoadState::Failed("unknown handle".into());
        };
        if let Some(error) = &slot.immediate_error {
            return AssetLoadState::Failed(error.clone());
        }
        let typed_handle = Handle::with_generation(handle.id, slot.generation);
        match self.store.state(typed_handle.clone()) {
            Ok(LoadStateView::Loading { .. }) => AssetLoadState::Loading,
            Ok(LoadStateView::Ready { .. }) => self
                .store
                .get(typed_handle)
                .map(AssetLoadState::Ready)
                .unwrap_or_else(|error| AssetLoadState::Failed(error.to_string())),
            Ok(LoadStateView::Failed { error, .. }) => AssetLoadState::Failed(error.to_string()),
            Err(error) => AssetLoadState::Failed(error.to_string()),
        }
    }

    /// Return the path associated with a handle, if any.
    pub fn path<T>(&self, handle: AsyncHandle<T>) -> Option<&Path> {
        self.slots.get(&handle.id).map(|slot| slot.path.as_path())
    }

    fn relative_path(&self, path: &Path) -> Result<PathBuf, AssetError> {
        if path.is_absolute() {
            let relative = path
                .strip_prefix(&self.project_root)
                .map_err(|_| AssetError::PathEscape)?;
            Ok(relative.to_path_buf())
        } else if self.project_root == filesystem_root() {
            let absolute = std::env::current_dir()
                .map_err(|_| AssetError::PathEscape)?
                .join(path);
            absolute
                .strip_prefix(&self.project_root)
                .map(Path::to_path_buf)
                .map_err(|_| AssetError::PathEscape)
        } else {
            Ok(path.to_path_buf())
        }
    }

    fn failed_handle<T: 'static>(&mut self, path: PathBuf, error: String) -> AsyncHandle<T> {
        let id = self.next_failed_id;
        self.next_failed_id = self.next_failed_id.saturating_sub(1);
        self.slots.insert(
            id,
            AssetSlot {
                path,
                generation: 0,
                last_modified: None,
                reload: reload_unavailable,
                immediate_error: Some(error),
            },
        );
        AsyncHandle::new(id)
    }
}

fn reload_typed<T: TypedAsset>(
    store: &mut AssetStore,
    id: u64,
    generation: u32,
) -> Result<ReloadTicket, AssetError> {
    store.reload(Handle::<T>::with_generation(id, generation))
}

fn reload_unavailable(
    _store: &mut AssetStore,
    _id: u64,
    _generation: u32,
) -> Result<ReloadTicket, AssetError> {
    Err(AssetError::WorkerStopped)
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

pub(super) fn filesystem_root() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut root = PathBuf::new();
    for component in current_dir.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => root.push(component.as_os_str()),
            Component::CurDir | Component::ParentDir | Component::Normal(_) => break,
        }
    }
    if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    }
}

/// Compatibility: turn an async handle into the synchronous `Handle<T>` once
/// the initial asset version is ready. Reloaded versions need a fresh typed handle.
pub fn to_sync_handle<T: 'static>(handle: AsyncHandle<T>) -> Handle<T> {
    Handle::new(handle.id)
}

#[cfg(test)]
#[path = "async_loader_tests.rs"]
mod tests;
