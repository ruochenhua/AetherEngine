//! GPU texture cache.
//!
//! Converts CPU `CpuTexture` assets uploaded through `AssetManager` into
//! GPU `GpuTexture` resources on demand and keeps them resident for the
//! lifetime of the cache.

use super::texture::{CpuTexture, GpuTexture};
use super::{AssetManager, Handle};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Cache that maps `Handle<CpuTexture>` to uploaded `GpuTexture` resources.
///
/// Uses interior mutability so the cache can be shared immutably across passes.
#[derive(Debug)]
pub struct GpuTextureCache {
    device: wgpu::Device,
    queue: wgpu::Queue,
    map: RwLock<HashMap<u64, Arc<GpuTexture>>>,
    fallback_white: Arc<GpuTexture>,
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
        let id = handle.id();
        {
            let map = self.map.read().expect("texture cache lock poisoned");
            if let Some(gpu) = map.get(&id) {
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
                tracing::warn!("CpuTexture handle {} not found in AssetManager", id);
                return self.fallback_white.clone();
            }
        };

        let mut map = self.map.write().expect("texture cache lock poisoned");
        map.insert(id, gpu.clone());
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
}

#[cfg(test)]
impl AssetManager {
    /// Test helper: insert a pre-built asset under a fake path.
    fn load_from(&mut self, path: &str, asset: CpuTexture) -> anyhow::Result<Handle<CpuTexture>> {
        let path = std::path::PathBuf::from(path);
        if let Some(&id) = self.path_to_id.get(&path) {
            return Ok(Handle::new(id));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.assets.insert(id, Arc::new(asset));
        self.path_to_id.insert(path, id);
        Ok(Handle::new(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn fallback_white_is_one_by_one() {
        let (device, queue) = headless_device_queue();
        let cache = GpuTextureCache::new(&device, &queue);
        let fallback = cache.fallback_white();
        assert_eq!(fallback.width, 1);
        assert_eq!(fallback.height, 1);
    }

    #[test]
    fn missing_handle_returns_fallback() {
        let (device, queue) = headless_device_queue();
        let cache = GpuTextureCache::new(&device, &queue);
        let assets = AssetManager::new();
        let handle = Handle::<CpuTexture>::new(999);
        let gpu = cache.get_or_upload(handle, &assets);
        assert_eq!(gpu.width, 1);
        assert_eq!(gpu.height, 1);
    }

    #[test]
    fn cached_textures_are_identical() {
        let (device, queue) = headless_device_queue();
        let mut assets = AssetManager::new();
        let cpu = CpuTexture::from_color(255, 0, 0, 255);
        // Manually insert the asset to bypass file loading.
        let handle = assets.load_from("test_red", cpu).unwrap();
        let cache = GpuTextureCache::new(&device, &queue);
        let a = cache.get_or_upload(handle.clone(), &assets);
        let b = cache.get_or_upload(handle, &assets);
        assert!(Arc::ptr_eq(&a, &b));
    }
}
