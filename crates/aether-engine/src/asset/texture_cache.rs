//! GPU texture cache.
//!
//! Converts typed CPU textures into GPU resources at frame boundaries and keeps
//! them keyed by persistent asset identity and generation.

use super::store::{AssetError, AssetId, AssetStore, FrameBoundary, ReloadTicket};
use super::texture::{CpuTexture, GpuTexture};
use super::{AssetManager, Handle};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread::ThreadId;

/// Cache that maps `(AssetId, generation)` to uploaded `GpuTexture` resources.
///
/// Uses interior mutability so the cache can be shared immutably across passes.
#[derive(Debug)]
pub struct GpuTextureCache {
    device: wgpu::Device,
    queue: wgpu::Queue,
    map: RwLock<HashMap<(AssetId, u32), Arc<GpuTexture>>>,
    fallback_white: Arc<GpuTexture>,
    owner_thread: ThreadId,
}

impl GpuTextureCache {
    /// Create a new GPU texture cache with a built-in 1x1 white fallback texture.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let fallback_white = Arc::new(GpuTexture::from_cpu(
            device,
            queue,
            &CpuTexture::from_color(255, 255, 255, 255),
            Some("fallback_white_texture"),
        ));
        Self {
            device: device.clone(),
            queue: queue.clone(),
            map: RwLock::new(HashMap::new()),
            fallback_white,
            owner_thread: std::thread::current().id(),
        }
    }

    /// Get or upload the GPU texture for the given CPU texture handle.
    ///
    /// Returns the fallback white texture if the handle is missing or the
    /// underlying CPU asset is not found.
    pub fn get_or_upload(
        &self,
        handle: Handle<CpuTexture>,
        assets: &AssetManager,
    ) -> Arc<GpuTexture> {
        if std::thread::current().id() != self.owner_thread {
            tracing::warn!("GPU texture lookup requested outside the cache owner thread");
            return self.fallback_white.clone();
        }
        let Ok(asset_id) = assets.asset_id(handle.clone()) else {
            tracing::warn!(
                "CpuTexture handle {} not found in AssetManager",
                handle.id()
            );
            return self.fallback_white.clone();
        };
        let key = cache_key(asset_id, &handle);
        {
            let map = self.map.read().expect("texture cache lock poisoned");
            if let Some(gpu) = map.get(&key) {
                return gpu.clone();
            }
        }

        let gpu = match assets.get(handle) {
            Some(cpu) => Arc::new(GpuTexture::from_cpu(
                &self.device,
                &self.queue,
                &cpu,
                Some("texture_cache"),
            )),
            None => {
                tracing::warn!("CpuTexture asset {} not found in AssetManager", key.0);
                return self.fallback_white.clone();
            }
        };

        let mut map = self.map.write().expect("texture cache lock poisoned");
        map.insert(key, gpu.clone());
        gpu
    }

    /// Get or upload a GPU texture from an optional handle.
    ///
    /// If the handle is `None`, returns the fallback white texture.
    pub fn get_or_upload_optional(
        &self,
        handle: Option<Handle<CpuTexture>>,
        assets: &AssetManager,
    ) -> Arc<GpuTexture> {
        match handle {
            Some(h) => self.get_or_upload(h, assets),
            None => self.fallback_white.clone(),
        }
    }

    /// Return the fallback white 1x1 texture.
    pub fn fallback_white(&self) -> Arc<GpuTexture> {
        self.fallback_white.clone()
    }

    /// Upload and publish a staged CPU texture at the requested frame boundary.
    /// The store checks its owner thread before invoking this renderer callback.
    pub fn commit_texture(
        &self,
        store: &mut AssetStore,
        ticket: &ReloadTicket,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        store.commit_result(ticket, boundary, |asset, generation, payload| {
            if asset.kind != super::store::AssetKind::CpuTexture {
                return Err(AssetError::PayloadKindMismatch);
            }
            let cpu = payload
                .downcast_ref::<CpuTexture>()
                .ok_or(AssetError::PayloadTypeMismatch)?;
            let gpu = Arc::new(GpuTexture::from_cpu(
                &self.device,
                &self.queue,
                cpu,
                Some("asset_frame_commit"),
            ));
            let mut map = self
                .map
                .write()
                .map_err(|_| AssetError::CommitFailed("texture cache lock poisoned".into()))?;
            map.retain(|(cached_asset, _), _| cached_asset != asset);
            map.insert((asset.clone(), generation), gpu);
            Ok(())
        })
    }
}

impl AssetManager {
    /// Commit a staged texture using this manager's store at a frame boundary.
    pub fn commit_staged_texture(
        &mut self,
        cache: &GpuTextureCache,
        ticket: &ReloadTicket,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        cache.commit_texture(&mut self.store, ticket, boundary)
    }
}

fn cache_key(asset_id: AssetId, handle: &Handle<CpuTexture>) -> (AssetId, u32) {
    (asset_id, handle.generation())
}

#[cfg(test)]
impl AssetManager {
    /// Test helper: insert a pre-built asset under a fake path.
    fn load_from(&mut self, path: &str, asset: CpuTexture) -> anyhow::Result<Handle<CpuTexture>> {
        self.store
            .insert_ready(std::path::Path::new(path), asset)
            .map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "texture_cache_tests.rs"]
mod tests;
