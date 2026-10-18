use super::*;
use crate::asset::{AssetId, AssetKind, GpuAssetKey};
use std::path::Path;

fn device_queue() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless adapter required");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("headless device required")
}

fn key(path: &str, generation: u32) -> GpuAssetKey {
    GpuAssetKey {
        asset: AssetId::from_path(
            AssetKind::CpuTexture,
            &std::env::current_dir().unwrap(),
            Path::new(path),
        )
        .unwrap(),
        generation,
    }
}

fn texture(device: &wgpu::Device, queue: &wgpu::Queue) -> Arc<GpuTexture> {
    Arc::new(GpuTexture::from_cpu(
        device,
        queue,
        &CpuTexture::from_color(10, 20, 30, 255),
        None,
    ))
}

#[test]
fn snapshot_lists_sorted_current_and_retired_generations_and_color_modes() {
    let (device, queue) = device_queue();
    let cache = GpuTextureCache::new(&device, &queue);
    let old = key("textures/shared.png", 1);
    let current = key("textures/shared.png", 2);
    let retained_old = texture(&device, &queue);
    {
        let mut state = cache.versioned.write().unwrap();
        state
            .assets
            .insert((old.clone(), false), retained_old.clone());
        state.current.insert(old.asset.clone(), 2);
        state.current.insert(current.asset.clone(), 2);
        state
            .assets
            .insert((current.clone(), true), texture(&device, &queue));
        state
            .assets
            .insert((current, false), texture(&device, &queue));
        state
            .assets
            .insert((key("textures/b.png", 1), false), texture(&device, &queue));
        state.current.insert(key("textures/b.png", 1).asset, 1);
    }

    let snapshot = cache.resource_usage_snapshot();
    assert_eq!(
        snapshot.state,
        crate::asset::GpuCacheInventoryState::Available
    );
    assert_eq!(snapshot.resources.len(), 4);
    assert!(snapshot.resources.windows(2).all(|pair| {
        (pair[0].asset.path(), pair[0].generation, pair[0].srgb)
            <= (pair[1].asset.path(), pair[1].generation, pair[1].srgb)
    }));
    let modes = snapshot
        .resources
        .iter()
        .filter(|entry| {
            entry.asset.path().as_str() == "textures/shared.png" && entry.generation == 2
        })
        .map(|entry| entry.srgb)
        .collect::<Vec<_>>();
    assert_eq!(modes, vec![Some(false), Some(true)]);
    let old_entry = snapshot
        .resources
        .iter()
        .find(|entry| entry.asset.path().as_str() == "textures/shared.png" && entry.generation == 1)
        .unwrap();
    assert!(!old_entry.current);
    assert_eq!(old_entry.external_arc_owners, 1);
    assert_eq!(snapshot.unidentified_legacy_texture_entries, Some(0));
    assert_eq!(cache.resource_usage_snapshot(), snapshot);
    assert_eq!(Arc::strong_count(&retained_old), 2);
}

#[test]
fn legacy_entries_and_poisoned_lock_are_visible_as_unknown_state() {
    let (device, queue) = device_queue();
    let cache = GpuTextureCache::new(&device, &queue);
    cache
        .map
        .write()
        .unwrap()
        .insert((42, false), texture(&device, &queue));
    let snapshot = cache.resource_usage_snapshot();
    assert_eq!(snapshot.unidentified_legacy_texture_entries, Some(1));

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = cache.versioned.write().unwrap();
        panic!("poison versioned map for inventory test");
    }));
    let snapshot = cache.resource_usage_snapshot();
    assert_eq!(
        snapshot.state,
        crate::asset::GpuCacheInventoryState::Unavailable
    );
    assert_eq!(snapshot.unidentified_legacy_texture_entries, None);
}
