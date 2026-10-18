use super::{GpuMaterialCache, MaterialAsset};
use crate::asset::AssetManager;
use crate::asset::{AssetId, AssetKind, AssetPayload, FrameBoundary, GpuAssetKey, GpuCommitCache};
use crate::scene::config::MaterialConfig;
use std::path::Path;
use std::sync::Arc;

fn device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless adapter required for GPU material lifetime test");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("headless device required for GPU material lifetime test")
        .0
}

fn payload(albedo: [f32; 4], root: &Path) -> Arc<dyn AssetPayload> {
    let mut material = MaterialAsset::from_config(MaterialConfig {
        albedo,
        ..MaterialConfig::default()
    });
    material
        .resolve(
            &AssetId::from_path(AssetKind::Material, root, Path::new("materials/paint.ron"))
                .unwrap(),
            root,
            &mut AssetManager::with_project_root(root),
        )
        .unwrap();
    Arc::new(material)
}

#[test]
fn old_gpu_material_lives_until_consumers_release_it() {
    let device = device();
    let mut cache = GpuMaterialCache::new(&device);
    let root = std::env::current_dir().unwrap();
    let id = AssetId::from_path(
        AssetKind::Material,
        &root,
        Path::new("scenes/material-lifetime.ron"),
    )
    .unwrap();
    let generation_one = GpuAssetKey {
        asset: id.clone(),
        generation: 1,
    };
    let generation_two = GpuAssetKey {
        asset: id,
        generation: 2,
    };

    cache
        .commit_asset(
            generation_one.clone(),
            payload([1.0, 0.0, 0.0, 1.0], &root),
            FrameBoundary::new(5),
        )
        .unwrap();
    let held_generation = cache.get_versioned(&generation_one).unwrap();
    cache
        .commit_asset(
            generation_two,
            payload([0.0, 1.0, 0.0, 1.0], &root),
            FrameBoundary::new(6),
        )
        .unwrap();
    assert!(cache.get_versioned(&generation_one).is_some());
    drop(held_generation);
    cache.collect_retired();
    assert!(cache.get_versioned(&generation_one).is_none());
}

#[test]
fn resource_usage_snapshot_lists_current_and_retained_retired_materials() {
    let device = device();
    let root = std::env::current_dir().unwrap();
    let mut cache = GpuMaterialCache::new(&device);
    let id =
        AssetId::from_path(AssetKind::Material, &root, Path::new("materials/paint.ron")).unwrap();
    let old = GpuAssetKey {
        asset: id.clone(),
        generation: 1,
    };
    let current = GpuAssetKey {
        asset: id,
        generation: 2,
    };
    cache
        .commit_asset(
            old.clone(),
            payload([1.0, 0.0, 0.0, 1.0], &root),
            FrameBoundary::new(5),
        )
        .unwrap();
    let retained = cache.get_versioned(&old).unwrap();
    cache
        .commit_asset(
            current,
            payload([0.0, 1.0, 0.0, 1.0], &root),
            FrameBoundary::new(6),
        )
        .unwrap();

    let snapshot = cache.resource_usage_snapshot();
    assert_eq!(
        snapshot.state,
        crate::asset::GpuCacheInventoryState::Available
    );
    assert_eq!(snapshot.unidentified_legacy_texture_entries, None);
    assert_eq!(snapshot.resources.len(), 2);
    assert!(snapshot
        .resources
        .iter()
        .all(|entry| entry.owner == crate::asset::GpuResourceOwner::Material));
    assert!(snapshot.resources.iter().all(|entry| entry.srgb.is_none()));
    assert!(!snapshot.resources[0].current);
    assert!(snapshot.resources[1].current);
    assert_eq!(snapshot.resources[0].external_arc_owners, 1);
    assert_eq!(cache.resource_usage_snapshot(), snapshot);
    assert_eq!(Arc::strong_count(&retained), 2);
    drop(retained);
}
