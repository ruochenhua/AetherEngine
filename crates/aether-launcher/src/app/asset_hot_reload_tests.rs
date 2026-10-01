use super::*;
use aether_engine::asset::prefab::{
    PrefabDocument, PrefabInstanceConfig, PrefabNode, PrefabOverrides, PREFAB_SCHEMA_VERSION,
};
use aether_engine::asset::{AssetError, AssetManager, AssetPayload, FrameBoundary, GpuAssetKey};
use aether_engine::ecs::components::{
    Name, PrefabAssetStatus, PrefabInstanceRoot, PrefabNodeInstance, Transform,
};
use aether_engine::ecs::World;
use aether_engine::editor::ComponentRecord;
use aether_engine::scene::MaterialConfig;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aether_launcher_hot_reload_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("prefabs")).unwrap();
        Self(root)
    }

    fn write_prefab(&self, name: &str) {
        document(name)
            .save(&self.0.join("prefabs/sample.ron"))
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn document(name: &str) -> PrefabDocument {
    PrefabDocument {
        schema_version: PREFAB_SCHEMA_VERSION,
        root: PrefabNode {
            instance_id: 4,
            name: name.into(),
            components: vec![
                ComponentRecord::Transform {
                    translation: [0.0, 0.0, 0.0],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                ComponentRecord::Name { value: name.into() },
            ],
            children: Vec::new(),
        },
    }
}

#[derive(Default)]
struct FakeGpuCache(Vec<GpuAssetKey>);

impl GpuCommitCache for FakeGpuCache {
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        _payload: Arc<dyn AssetPayload>,
        _boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        self.0.push(key);
        Ok(())
    }

    fn collect_retired(&mut self) {}
}

#[test]
fn prefab_file_change_replaces_the_instance_after_successful_reload() {
    let fixture = Fixture::new();
    fixture.write_prefab("old value");
    let mut assets = AssetManager::with_project_root(&fixture.0);
    assets.load::<PrefabAsset>("prefabs/sample.ron").unwrap();
    let instance = PrefabInstanceConfig {
        instance_id: 88,
        prefab_asset: "prefabs/sample.ron".into(),
        overrides: PrefabOverrides::default(),
    };
    let mut world = World::new();
    let old_entity = world.spawn((
        PrefabNodeInstance {
            prefab_instance_id: instance.instance_id,
            instance_id: 4,
            parent_instance_id: None,
        },
        PrefabInstanceRoot(instance),
        Transform::default(),
        Name("old value".into()),
    ));
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let request =
        pollster::block_on(adapter.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless adapter required for Prefab replacement test");
    let (device, _queue) =
        pollster::block_on(request.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("headless device required for Prefab replacement test");
    let registry = BuiltinMeshRegistry::new();
    let mut cache = FakeGpuCache::default();
    let mut watcher = AssetHotReloadWatcher::default();

    watcher.update(&mut world, &mut assets, &mut cache, &device, &registry);
    std::thread::sleep(Duration::from_millis(80));
    fixture.write_prefab("new value");
    watcher.update(&mut world, &mut assets, &mut cache, &device, &registry);

    for _ in 0..200 {
        watcher.update(&mut world, &mut assets, &mut cache, &device, &registry);
        if world
            .query::<&Name>()
            .iter()
            .any(|name| name.0 == "new value")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!world.contains(old_entity));
    assert_eq!(
        world.query::<&PrefabNodeInstance>().iter().count(),
        1,
        "successful reload should leave exactly one replacement node"
    );
    assert!(world
        .query::<&Name>()
        .iter()
        .any(|name| name.0 == "new value"));
    assert!(cache.0.is_empty());

    std::thread::sleep(Duration::from_millis(80));
    fs::write(
        fixture.0.join("prefabs/sample.ron"),
        "invalid Prefab document",
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut diagnostic_visible = false;
    while std::time::Instant::now() < deadline {
        watcher.update(&mut world, &mut assets, &mut cache, &device, &registry);
        diagnostic_visible = world
            .query::<&PrefabAssetStatus>()
            .iter()
            .any(|status| status.0.is_some());
        if diagnostic_visible {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        diagnostic_visible,
        "Prefab reload failure should expose a UI diagnostic component"
    );
    assert!(world
        .query::<&Name>()
        .iter()
        .any(|name| name.0 == "new value"));
    assets.shutdown().unwrap();
}

#[test]
fn changed_material_texture_commits_a_new_cpu_generation_and_refreshes_material() {
    let fixture = Fixture::new();
    let material_dir = fixture.0.join("materials");
    let texture_dir = material_dir.join("textures");
    fs::create_dir_all(&texture_dir).unwrap();
    fs::write(
        material_dir.join("paint.ron"),
        "(albedo_texture: Some(\"textures/paint.png\"), albedo: (1.0, 1.0, 1.0, 1.0), roughness: 0.5, metallic: 0.0)",
    )
    .unwrap();
    write_png(&texture_dir.join("paint.png"), [180, 50, 30, 255]);

    let mut assets = AssetManager::with_project_root(&fixture.0);
    assets
        .request::<MaterialAsset>("materials/paint.ron")
        .unwrap();
    let mut frame = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut initial_ready = false;
    while std::time::Instant::now() < deadline && !initial_ready {
        for result in assets.poll_results(8).unwrap() {
            frame += 1;
            let outcome = assets
                .apply_material_result(result, FrameBoundary::new(frame))
                .unwrap();
            initial_ready |= matches!(outcome.state, LoadStateView::Ready { .. });
        }
        if !initial_ready {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(initial_ready, "material and texture fixture did not load");

    let mut world = World::new();
    world.spawn((
        MaterialAssetRef("materials/paint.ron".into()),
        MaterialAssetStatus::default(),
        MaterialConfig::default(),
    ));
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let request =
        pollster::block_on(adapter.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless adapter required for texture reload test");
    let (device, _queue) =
        pollster::block_on(request.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("headless device required for texture reload test");
    let registry = BuiltinMeshRegistry::new();
    let mut cache = FakeGpuCache::default();
    let mut hot_reload = AssetHotReloadWatcher::default();
    let mut material_reload = crate::app::material_reload::MaterialAssetWatcher::default();
    hot_reload.update(&mut world, &mut assets, &mut cache, &device, &registry);
    material_reload.update(&mut world, &mut assets);

    std::thread::sleep(Duration::from_millis(80));
    write_png(&texture_dir.join("paint.png"), [30, 80, 220, 255]);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut refreshed = false;
    while std::time::Instant::now() < deadline {
        hot_reload.update(&mut world, &mut assets, &mut cache, &device, &registry);
        material_reload.update(&mut world, &mut assets);
        let material = assets
            .load::<MaterialAsset>("materials/paint.ron")
            .ok()
            .and_then(|handle| assets.get(handle));
        refreshed = material
            .and_then(|asset| asset.resolution().cloned())
            .and_then(|resolution| resolution.material.albedo)
            .is_some_and(|handle| handle.generation() > 1);
        if refreshed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let texture_id = AssetId::from_path(
        AssetKind::CpuTexture,
        &fixture.0,
        Path::new("materials/textures/paint.png"),
    )
    .unwrap();
    assert!(
        refreshed,
        "material did not bind the reloaded texture generation"
    );
    assert!(cache
        .0
        .iter()
        .any(|key| key.asset == texture_id && key.generation == 2));
    assets.shutdown().unwrap();
}

fn write_png(path: &std::path::Path, rgba: [u8; 4]) {
    image::RgbaImage::from_pixel(1, 1, image::Rgba(rgba))
        .save(path)
        .unwrap();
}
