use super::*;
use crate::asset::store::{AssetStore, AssetStoreConfig, FrameBoundary};
use std::thread;
use std::time::Duration;

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

#[test]
fn gpu_cache_identity_includes_asset_generation() {
    let old_version = Handle::<CpuTexture>::with_generation(17, 1);
    let new_version = Handle::<CpuTexture>::with_generation(17, 2);
    let asset_id = super::super::store::AssetId {
        kind: super::super::store::AssetKind::CpuTexture,
        path: super::super::store::canonicalize(
            std::path::Path::new("."),
            std::path::Path::new("textures/albedo.png"),
        )
        .unwrap(),
    };
    assert_ne!(
        cache_key(asset_id.clone(), &old_version),
        cache_key(asset_id, &new_version)
    );
}

#[test]
fn staged_texture_upload_is_published_only_by_frame_boundary_commit() {
    let root = std::env::temp_dir().join(format!("aether_gpu_asset_commit_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let image_path = root.join("texture.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([16, 32, 64, 255]))
        .save(&image_path)
        .unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, _) = store
        .request::<CpuTexture>(std::path::Path::new("texture.png"))
        .unwrap();
    let ticket = loop {
        if let Some(result) = store.poll_results(1).unwrap().pop() {
            break store
                .apply_result(result, FrameBoundary { frame_id: 1 })
                .unwrap();
        }
        thread::sleep(Duration::from_millis(2));
    };
    assert!(store.get(handle.clone()).is_err());

    let (device, queue) = headless_device_queue();
    let cache = GpuTextureCache::new(&device, &queue);
    cache
        .commit_texture(&mut store, &ticket, FrameBoundary { frame_id: 1 })
        .unwrap();
    assert_eq!(store.get(handle).unwrap().width, 1);
    assert!(cache
        .map
        .read()
        .unwrap()
        .contains_key(&(ticket.asset.clone(), ticket.to_generation)));
    std::fs::remove_dir_all(root).unwrap();
}
