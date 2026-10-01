//! Frame-boundary GPU storage for versioned material assets.

use super::MaterialAsset;
use crate::asset::{
    AssetError, AssetId, AssetKind, AssetPayload, FrameBoundary, GpuAssetKey, GpuCommitCache,
};
use crate::renderer::renderable::MaterialUniform;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;

/// GPU uniform buffer and retained CPU material for one committed generation.
pub struct GpuMaterial {
    /// Uniform values used to create the resource.
    pub uniform: MaterialUniform,
    /// Versioned GPU material buffer.
    pub uniform_buffer: wgpu::Buffer,
    _source: Arc<dyn AssetPayload>,
}

#[derive(Default)]
struct CacheState {
    assets: HashMap<GpuAssetKey, Arc<GpuMaterial>>,
    current: HashMap<AssetId, u32>,
    last_frame: Option<u64>,
}

/// Cache that installs material GPU resources only at caller-owned frame boundaries.
pub struct GpuMaterialCache {
    device: wgpu::Device,
    state: CacheState,
}

impl GpuMaterialCache {
    /// Create an empty material cache for a device.
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            state: CacheState::default(),
        }
    }

    /// Return a specific committed material generation.
    pub fn get_versioned(&self, key: &GpuAssetKey) -> Option<Arc<GpuMaterial>> {
        self.state.assets.get(key).cloned()
    }
}

impl GpuCommitCache for GpuMaterialCache {
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        payload: Arc<dyn AssetPayload>,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        if key.asset.kind() != AssetKind::Material {
            return Err(AssetError::UnsupportedKind(key.asset.kind()));
        }
        if self
            .state
            .last_frame
            .is_some_and(|frame| boundary.frame_id < frame)
        {
            return Err(AssetError::InvalidFrameBoundary);
        }
        if self
            .state
            .current
            .get(&key.asset)
            .is_some_and(|generation| *generation > key.generation)
        {
            return Err(AssetError::StaleReloadTicket);
        }
        if self.state.assets.contains_key(&key) {
            self.state.last_frame = Some(boundary.frame_id);
            return Ok(());
        }
        let material = payload
            .as_any()
            .downcast_ref::<MaterialAsset>()
            .ok_or(AssetError::PayloadTypeMismatch)?;
        let resolution = material.resolution().ok_or(AssetError::NotReady)?;
        let uniform = MaterialUniform::from_resolution(resolution);
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("asset_store_material_uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let gpu = Arc::new(GpuMaterial {
            uniform,
            uniform_buffer,
            _source: payload,
        });
        self.state.current.insert(key.asset.clone(), key.generation);
        self.state.assets.insert(key, gpu);
        self.state.last_frame = Some(boundary.frame_id);
        self.collect_retired();
        Ok(())
    }

    fn collect_retired(&mut self) {
        let current = &self.state.current;
        self.state.assets.retain(|key, material| {
            current.get(&key.asset) == Some(&key.generation) || Arc::strong_count(material) > 1
        });
    }
}
