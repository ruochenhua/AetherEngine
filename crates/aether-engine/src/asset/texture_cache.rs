//! GPU texture cache facade and tests.

#[path = "texture_cache/cache.rs"]
mod cache;

pub use cache::GpuTextureCache;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::texture::CpuTexture;
    use crate::asset::{AssetManager, Handle};
    use std::sync::Arc;

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
        let handle = assets.load_from("test_red", cpu).unwrap();
        let cache = GpuTextureCache::new(&device, &queue);
        let a = cache.get_or_upload(handle, &assets);
        let b = cache.get_or_upload(handle, &assets);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn color_and_data_slots_upload_with_distinct_gpu_formats() {
        let (device, queue) = headless_device_queue();
        let mut assets = AssetManager::new();
        let handle = assets
            .load_from("shared_texture", CpuTexture::from_color(180, 90, 45, 255))
            .unwrap();
        let cache = GpuTextureCache::new(&device, &queue);

        let color = cache.get_or_upload_with_srgb(handle, &assets, true);
        let data = cache.get_or_upload_with_srgb(handle, &assets, false);

        assert_eq!(color.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(data.format, wgpu::TextureFormat::Rgba8Unorm);
        assert!(!Arc::ptr_eq(&color, &data));
    }
}
