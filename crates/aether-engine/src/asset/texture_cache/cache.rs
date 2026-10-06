use crate::asset::texture::{CpuTexture, GpuTexture};
use crate::asset::{
    AssetError, AssetId, AssetManager, AssetPayload, FrameBoundary, GpuAssetKey, GpuCommitCache,
    Handle,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Cache mapping CPU texture handles and generations to GPU textures.
#[derive(Debug)]
pub struct GpuTextureCache {
    device: wgpu::Device,
    queue: wgpu::Queue,
    map: RwLock<HashMap<(u64, bool), Arc<GpuTexture>>>,
    versioned: RwLock<VersionedCache>,
    fallback_white: Arc<GpuTexture>,
}

#[derive(Debug, Default)]
struct VersionedCache {
    assets: HashMap<(GpuAssetKey, bool), Arc<GpuTexture>>,
    current: HashMap<AssetId, u32>,
    last_frame: Option<u64>,
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
            versioned: RwLock::new(VersionedCache::default()),
            fallback_white,
        }
    }

    /// Get or upload the GPU texture for the given CPU texture handle.
    pub fn get_or_upload(
        &self,
        handle: Handle<CpuTexture>,
        assets: &AssetManager,
    ) -> Arc<GpuTexture> {
        self.get_or_upload_with_srgb(handle, assets, false)
    }

    /// Get or upload the GPU texture using an explicit color interpretation.
    pub fn get_or_upload_with_srgb(
        &self,
        handle: Handle<CpuTexture>,
        assets: &AssetManager,
        srgb: bool,
    ) -> Arc<GpuTexture> {
        let id = handle.id();
        let versioned_key = assets.asset_id(handle).ok().map(|asset| GpuAssetKey {
            asset,
            generation: handle.generation(),
        });
        if let Some(key) = &versioned_key {
            if let Some(gpu) = self.get_versioned_with_srgb(key, srgb) {
                return gpu;
            }
        }
        {
            let map = self.map.read().expect("texture cache lock poisoned");
            if let Some(gpu) = map.get(&(id, srgb)) {
                return gpu.clone();
            }
        }

        let gpu = match assets.get(handle) {
            Some(cpu) => Arc::new(GpuTexture::from_cpu_with_srgb(
                &self.device,
                &self.queue,
                &cpu,
                Some("texture_cache"),
                srgb,
            )),
            None => {
                tracing::warn!("CpuTexture handle {} not found in AssetManager", id);
                return self.fallback_white.clone();
            }
        };

        let mut map = self.map.write().expect("texture cache lock poisoned");
        map.insert((id, srgb), gpu.clone());
        if let Some(key) = versioned_key {
            if let Ok(mut cache) = self.versioned.write() {
                cache.current.insert(key.asset.clone(), key.generation);
                cache.assets.insert((key, srgb), gpu.clone());
                collect_unreferenced(&mut cache);
            }
        }
        gpu
    }

    /// Get or upload a GPU texture from an optional handle.
    pub fn get_or_upload_optional(
        &self,
        handle: Option<Handle<CpuTexture>>,
        assets: &AssetManager,
    ) -> Arc<GpuTexture> {
        self.get_or_upload_optional_with_srgb(handle, assets, false)
    }

    /// Get or upload an optional texture with an explicit color interpretation.
    pub fn get_or_upload_optional_with_srgb(
        &self,
        handle: Option<Handle<CpuTexture>>,
        assets: &AssetManager,
        srgb: bool,
    ) -> Arc<GpuTexture> {
        match handle {
            Some(handle) => self.get_or_upload_with_srgb(handle, assets, srgb),
            None => self.fallback_white.clone(),
        }
    }

    /// Return the fallback white 1x1 texture.
    pub fn fallback_white(&self) -> Arc<GpuTexture> {
        self.fallback_white.clone()
    }

    /// Return a GPU texture by persistent identity and generation.
    pub fn get_versioned(&self, key: &GpuAssetKey) -> Option<Arc<GpuTexture>> {
        self.get_versioned_with_srgb(key, false)
    }

    /// Return a cached texture by asset generation and color interpretation.
    pub fn get_versioned_with_srgb(
        &self,
        key: &GpuAssetKey,
        srgb: bool,
    ) -> Option<Arc<GpuTexture>> {
        self.versioned
            .read()
            .ok()?
            .assets
            .get(&(key.clone(), srgb))
            .cloned()
    }
}

impl GpuCommitCache for GpuTextureCache {
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        payload: Arc<dyn AssetPayload>,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        if key.asset.kind() != crate::asset::AssetKind::CpuTexture {
            return Err(AssetError::UnsupportedKind(key.asset.kind()));
        }
        let cpu = payload
            .as_any()
            .downcast_ref::<CpuTexture>()
            .ok_or(AssetError::PayloadTypeMismatch)?;
        let mut cache = self
            .versioned
            .write()
            .map_err(|_| AssetError::GpuCommit("texture cache lock poisoned".into()))?;
        if cache
            .last_frame
            .is_some_and(|frame| boundary.frame_id < frame)
        {
            return Err(AssetError::InvalidFrameBoundary);
        }
        if cache
            .current
            .get(&key.asset)
            .is_some_and(|generation| *generation > key.generation)
        {
            return Err(AssetError::StaleReloadTicket);
        }
        if cache.assets.contains_key(&(key.clone(), false)) {
            cache.last_frame = Some(boundary.frame_id);
            return Ok(());
        }
        let gpu = Arc::new(GpuTexture::from_cpu(
            &self.device,
            &self.queue,
            cpu,
            Some("asset_store_texture"),
        ));
        cache.current.insert(key.asset.clone(), key.generation);
        cache.assets.insert((key, false), gpu);
        cache.last_frame = Some(boundary.frame_id);
        collect_unreferenced(&mut cache);
        Ok(())
    }

    fn collect_retired(&mut self) {
        if let Ok(mut cache) = self.versioned.write() {
            collect_unreferenced(&mut cache);
        }
    }
}

fn collect_unreferenced(cache: &mut VersionedCache) {
    let current = &cache.current;
    cache.assets.retain(|(key, _srgb), texture| {
        current.get(&key.asset) == Some(&key.generation) || Arc::strong_count(texture) > 1
    });
}
