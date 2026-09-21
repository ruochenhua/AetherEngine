use super::*;
use crate::ecs::components::Transform;
use crate::ecs::World;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    AtmosphereConfig, CameraConfig, LightConfig, MeshRef, ObjectConfig, TerrainConfig,
    TerrainGeometry, TerrainLayerConfig, TerrainSource, WaterConfig,
};
fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, _queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");
    device
}

fn test_registry() -> BuiltinMeshRegistry {
    BuiltinMeshRegistry::new()
}

fn test_assets() -> AssetManager {
    AssetManager::new()
}
fn test_scene_desc() -> SceneDescription {
    SceneDescription {
        name: "Test".into(),
        camera: CameraConfig::default(),
        lights: vec![LightConfig::default()],
        ambient: 0.05,
        terrain: None,
        atmosphere: None,
        water: None,
        clouds: None,
        god_ray: None,
        objects: vec![
            ObjectConfig {
                name: "cube_left".into(),
                mesh: MeshRef::Builtin("cube".into()),
                transform: crate::scene::TransformConfig {
                    translation: [-0.8, 0.0, 0.0],
                    ..Default::default()
                },
                material: crate::scene::MaterialConfig {
                    albedo: [0.8, 0.3, 0.2, 1.0],
                    roughness: 0.5,
                    metallic: 0.0,
                    unlit: false,
                    albedo_texture: None,
                    ..Default::default()
                },
                visible: true,
            },
            ObjectConfig {
                name: "sphere_right".into(),
                mesh: MeshRef::Builtin("sphere".into()),
                transform: crate::scene::TransformConfig {
                    translation: [0.8, 0.0, 0.0],
                    ..Default::default()
                },
                material: crate::scene::MaterialConfig {
                    albedo: [0.2, 0.5, 0.8, 1.0],
                    roughness: 0.05,
                    metallic: 0.0,
                    unlit: false,
                    albedo_texture: None,
                    ..Default::default()
                },
                visible: true,
            },
        ],
    }
}

#[test]
fn build_world_creates_correct_number_of_entities() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();

    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
        .expect("should build world");

    // camera + light + 2 objects = 4 entities
    assert_eq!(world.len(), 4);
}

#[test]
fn build_world_sets_correct_material() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.objects[0].material.normal_scale = 1.5;
    desc.objects[0].material.occlusion_strength = 0.4;
    desc.objects[0].material.emissive = [0.2, 0.3, 0.4];
    desc.objects[0].material.emissive_intensity = 2.0;
    desc.objects[0].material.unlit = true;
    let mut world = World::new();

    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let mut found = false;
    for material in world.query::<&MaterialUniform>().iter() {
        if material.albedo == [0.8, 0.3, 0.2, 1.0] {
            found = true;
            assert_eq!(material.roughness, 0.5);
            assert_eq!(material.normal_scale, 1.5);
            assert_eq!(material.occlusion_strength, 0.4);
            assert_eq!(material.emissive, [0.2, 0.3, 0.4]);
            assert_eq!(material.emissive_intensity, 2.0);
            assert_eq!(material.unlit, 1);
            break;
        }
    }
    assert!(found, "expected cube material in world");
}

#[test]
fn build_world_sets_correct_transform() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();

    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let mut found = false;
    for transform in world.query::<&Transform>().iter() {
        if (transform.translation - Vec3::new(0.8, 0.0, 0.0)).length() < 0.001 {
            found = true;
            break;
        }
    }
    assert!(found, "expected sphere transform in world");
}

#[test]
fn build_world_sets_lighting_uniforms() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();

    let mut assets = test_assets();
    let lighting =
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();
    assert_eq!(lighting.ambient_intensity, 0.05);
}

#[test]
fn build_world_spawns_camera_entity() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let camera_count = world.query::<&Camera>().iter().count();
    assert_eq!(camera_count, 1);
}

#[test]
fn build_world_spawns_light_entity() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let light_count = world.query::<&Light>().iter().count();
    assert_eq!(light_count, 1);
}

#[test]
fn build_world_spawns_all_configured_local_lights() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.lights = vec![
        LightConfig::default(),
        LightConfig {
            light_type: crate::renderer::light::LightType::Point,
            position: [-2.0, 2.0, 0.0],
            color: [1.0, 0.0, 0.0],
            range: 6.0,
            ..LightConfig::default()
        },
        LightConfig {
            light_type: crate::renderer::light::LightType::Point,
            position: [0.0, 2.0, 0.0],
            color: [0.0, 1.0, 0.0],
            range: 6.0,
            ..LightConfig::default()
        },
        LightConfig {
            light_type: crate::renderer::light::LightType::Point,
            position: [2.0, 2.0, 0.0],
            color: [0.0, 0.0, 1.0],
            range: 6.0,
            ..LightConfig::default()
        },
    ];
    let mut world = World::new();
    let mut assets = test_assets();

    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
        .expect("all configured lights should load");

    let colors: Vec<_> = world
        .query::<&Light>()
        .iter()
        .map(|light| light.color)
        .collect();
    assert_eq!(colors.len(), 4);
    assert!(colors.contains(&[1.0, 0.0, 0.0]));
    assert!(colors.contains(&[0.0, 1.0, 0.0]));
    assert!(colors.contains(&[0.0, 0.0, 1.0]));
}

#[test]
fn build_world_attaches_name_to_objects() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
    assert!(names.contains(&"cube_left".to_string()));
    assert!(names.contains(&"sphere_right".to_string()));
}

#[test]
fn build_world_attaches_name_to_scene_level_entities() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
    assert!(names.contains(&"Camera".to_string()));
    assert!(names.contains(&"DirectionalLight".to_string()));
}

#[test]
fn build_world_unknown_mesh_returns_error() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.objects[0].mesh = MeshRef::Builtin("nonexistent".into());
    let mut world = World::new();
    let mut assets = test_assets();

    let result = SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world);
    assert!(result.is_err());
}

#[test]
fn build_world_file_mesh_loads_obj() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();

    let dir = std::env::temp_dir().join("aether_test_file_mesh");
    let _ = std::fs::create_dir(&dir);
    let obj_path = dir.join("test_quad.obj");
    std::fs::write(
        &obj_path,
        "v 0.0 0.0 0.0\nv 1.0 0.0 0.0\nv 1.0 0.0 1.0\nv 0.0 0.0 1.0\nf 1 3 2\nf 1 4 3\n",
    )
    .unwrap();

    desc.objects[0].mesh = MeshRef::File(obj_path.to_string_lossy().to_string());
    let mut world = World::new();
    let mut assets = test_assets();

    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
        .expect("file mesh should load");

    // The original cube_left object is now a file mesh; both objects still spawn.
    assert_eq!(world.len(), 4);

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn open_scene_clears_world() {
    let device = headless_device();
    let registry = test_registry();
    let mut world = World::new();
    world.spawn((Transform::default(), Name("extra".into())));
    let mut assets = test_assets();

    let dir = std::env::temp_dir().join("aether_test_open_scene");
    let _ = std::fs::create_dir(&dir);
    let path = dir.join("open_scene.ron");
    std::fs::write(
        &path,
        r#"SceneDescription(
    name: "Open",
    camera: (position: (0.0, 0.0, 0.0)),
    lights: [],
    objects: [],
)"#,
    )
    .unwrap();

    SceneLoader::open_scene(&path, &device, &registry, &mut assets, &mut world).unwrap();
    // Only camera + default light should remain
    assert_eq!(world.len(), 2);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn open_scene_invalid_path_returns_error_without_panicking() {
    let device = headless_device();
    let registry = test_registry();
    let mut assets = test_assets();
    let mut world = World::new();

    let dir = std::env::temp_dir().join("aether_test_invalid_scene");
    let _ = std::fs::create_dir(&dir);
    let path = dir.join("broken.ron");
    std::fs::write(&path, "not a valid scene").unwrap();

    let result = SceneLoader::open_scene(&path, &device, &registry, &mut assets, &mut world);
    assert!(result.is_err(), "invalid scene should return an error");

    // The world should remain usable after a failed load.
    world.spawn((Transform::default(), Name("still-alive".into())));
    assert_eq!(world.len(), 1);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn import_scene_appends_objects() {
    let device = headless_device();
    let registry = test_registry();

    let dir = std::env::temp_dir().join("aether_test_import_scene");
    let _ = std::fs::create_dir(&dir);
    let path = dir.join("import_scene.ron");
    std::fs::write(
            &path,
            r#"SceneDescription(
    name: "Import",
    camera: (position: (1.0, 2.0, 3.0)),
    lights: [(light_type: Directional, direction: (0.0, -1.0, 0.0), color: (1.0, 1.0, 1.0), intensity: 1.0)],
    objects: [
        (name: "ImportedSphere", mesh: Builtin("sphere"), transform: (translation: (0.0, 0.0, 0.0)), material: (albedo: (1.0, 1.0, 1.0, 1.0))),
    ],
)"#,
        )
        .unwrap();

    let mut world = World::new();
    // Pre-populate with a camera and an object
    world.spawn((Transform::default(), Camera::default()));
    world.spawn((Transform::default(), Name("ExistingCube".into())));
    assert_eq!(world.len(), 2);

    let mut assets = test_assets();
    SceneLoader::import_scene(&path, &device, &registry, &mut assets, &mut world).unwrap();

    // 2 existing + 1 imported object = 3 entities (camera and light NOT imported)
    assert_eq!(world.len(), 3);

    let mut names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["ExistingCube", "ImportedSphere"]);

    // Verify existing camera is preserved
    let camera_count = world.query::<&Camera>().iter().count();
    assert_eq!(camera_count, 1);

    // cleanup
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn from_file_loads_valid_ron() {
    let dir = std::env::temp_dir().join("aether_test_scene");
    let _ = std::fs::create_dir(&dir);
    let path = dir.join("test_scene.ron");
    std::fs::write(
        &path,
        r#"SceneDescription(
    name: "From File",
    camera: (position: (1.0, 2.0, 3.0)),
)"#,
    )
    .unwrap();

    let desc = SceneLoader::from_file(&path).expect("should load");
    assert_eq!(desc.name, "From File");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn build_world_no_terrain_entity_when_missing() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let terrain_count = world
        .query::<&crate::ecs::components::Terrain>()
        .iter()
        .count();
    assert_eq!(terrain_count, 0);
}

#[test]
fn build_world_spawns_terrain_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.terrain = Some(TerrainConfig {
        source: TerrainSource::Procedural {
            seed: 0,
            frequency: 0.1,
            amplitude: 1.0,
        },
        geometry: TerrainGeometry {
            extent: 128.0,
            chunk_size: 32,
            max_lod: 2,
            albedo_tiling: 64.0,
        },
        splatmap: None,
        layers: vec![TerrainLayerConfig::default()],
    });
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let terrain_count = world
        .query::<&crate::ecs::components::Terrain>()
        .iter()
        .count();
    assert_eq!(terrain_count, 1);
}

#[test]
fn build_world_spawns_atmosphere_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.atmosphere = Some(AtmosphereConfig::default());
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Atmosphere>()
        .iter()
        .count();
    assert_eq!(count, 1);
}

#[test]
fn build_world_spawns_water_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.water = Some(WaterConfig::default());
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Water>()
        .iter()
        .count();
    assert_eq!(count, 1);
}
