use super::helpers::{light_direction_to_rotation, light_rotation_to_direction};
use super::*;
use aether_engine::ecs::components::{Light, Selected, Terrain, Transform};
use aether_engine::ecs::World;
use aether_engine::renderer::light::LightType;
use aether_engine::renderer::renderable::MaterialUniform;
use glam::Vec3;

fn world_with_light() -> (World, Entity) {
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        Light {
            light_type: LightType::Directional,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            range: 10.0,
            inner_cone_angle: 0.35,
            outer_cone_angle: 0.7,
            cast_shadow: true,
        },
        Selected,
    ));
    (world, entity)
}

#[test]
fn extract_returns_light_for_selected_light_entity() {
    let (world, entity) = world_with_light();
    let target = extract(&world).expect("should extract light target");
    assert_eq!(target.entity(), entity);
    match target {
        InspectorTarget::Light { direction, .. } => {
            assert!((direction[1] + 1.0).abs() < 1e-4);
        }
        _ => panic!("expected Light target"),
    }
}

#[test]
fn light_direction_roundtrip_through_rotation() {
    let direction = Vec3::new(0.2, -0.6, -0.8).normalize();
    let rotation = light_direction_to_rotation(direction);
    let recovered = light_rotation_to_direction(rotation);
    assert!((direction - recovered).length() < 1e-4);
}

#[test]
fn apply_light_updates_transform_rotation() {
    let (mut world, entity) = world_with_light();
    let mut target = extract(&world).unwrap();
    match &mut target {
        InspectorTarget::Light {
            direction, light, ..
        } => {
            *direction = [0.0, 0.0, -1.0];
            light.intensity = 2.5;
        }
        _ => panic!("expected Light target"),
    }
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    apply(
        &target,
        &mut world,
        &mut undo,
        &mut redo,
        &mut aether_engine::asset::AssetManager::new(),
    )
    .unwrap();

    let transform = world.query_one_mut::<&Transform>(entity).unwrap();
    let recovered = light_rotation_to_direction(transform.rotation);
    assert!((recovered - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-4);
    assert_eq!(
        world.query_one_mut::<&Light>(entity).unwrap().intensity,
        2.5
    );
}

#[test]
fn apply_terrain_rebuilds_material_layers() {
    let mut world = World::new();
    let terrain = Terrain {
        source: aether_engine::scene::TerrainSource::Procedural {
            seed: 1,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: aether_engine::scene::TerrainGeometry::default(),
        material: aether_engine::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![aether_engine::scene::TerrainLayerConfig {
            albedo: [1.0, 0.0, 0.0, 1.0],
            roughness: 0.1,
            metallic: 0.2,
            ..Default::default()
        }],
    };
    let entity = world.spawn((Transform::default(), terrain, Selected));
    let mut target = extract(&world).unwrap();
    match &mut target {
        InspectorTarget::Terrain { terrain, .. } => {
            terrain.layer_configs[0].albedo = [0.0, 1.0, 0.0, 1.0];
        }
        _ => panic!("expected Terrain target"),
    }
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    apply(
        &target,
        &mut world,
        &mut undo,
        &mut redo,
        &mut aether_engine::asset::AssetManager::new(),
    )
    .unwrap();

    let terrain = world.query_one_mut::<&Terrain>(entity).unwrap();
    assert_eq!(terrain.material.layers[0].albedo, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(terrain.material.layers[0].roughness, 0.1);
}

fn world_with_camera() -> (World, Entity) {
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        Camera {
            fov: 60.0f32.to_radians(),
            near: 0.1,
            far: 500.0,
            speed: 8.0,
        },
        Selected,
    ));
    (world, entity)
}

#[test]
fn extract_returns_camera_for_selected_camera_entity() {
    let (world, entity) = world_with_camera();
    let target = extract(&world).expect("should extract camera target");
    assert_eq!(target.entity(), entity);
    match target {
        InspectorTarget::Camera {
            camera,
            fov_degrees,
            ..
        } => {
            assert!((camera.fov - 60.0f32.to_radians()).abs() < 1e-4);
            assert!((fov_degrees - 60.0).abs() < 1e-4);
            assert_eq!(camera.speed, 8.0);
            assert_eq!(camera.near, 0.1);
            assert_eq!(camera.far, 500.0);
        }
        _ => panic!("expected Camera target"),
    }
}

#[test]
fn apply_camera_updates_component() {
    let (mut world, entity) = world_with_camera();
    let mut target = extract(&world).unwrap();
    match &mut target {
        InspectorTarget::Camera {
            camera,
            fov_degrees,
            ..
        } => {
            *fov_degrees = 90.0;
            camera.fov = 90.0f32.to_radians();
            camera.speed = 16.0;
            camera.near = 0.5;
            camera.far = 2000.0;
        }
        _ => panic!("expected Camera target"),
    }
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    apply(
        &target,
        &mut world,
        &mut undo,
        &mut redo,
        &mut aether_engine::asset::AssetManager::new(),
    )
    .unwrap();

    let camera = world.query_one_mut::<&Camera>(entity).unwrap();
    assert!((camera.fov - 90.0f32.to_radians()).abs() < 1e-4);
    assert_eq!(camera.speed, 16.0);
    assert_eq!(camera.near, 0.5);
    assert_eq!(camera.far, 2000.0);
}

#[test]
fn apply_camera_undo_restores_previous_values() {
    let (mut world, entity) = world_with_camera();
    let mut target = extract(&world).unwrap();
    match &mut target {
        InspectorTarget::Camera {
            camera,
            fov_degrees,
            ..
        } => {
            *fov_degrees = 90.0;
            camera.fov = 90.0f32.to_radians();
            camera.speed = 16.0;
        }
        _ => panic!("expected Camera target"),
    }
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    apply(
        &target,
        &mut world,
        &mut undo,
        &mut redo,
        &mut aether_engine::asset::AssetManager::new(),
    )
    .unwrap();
    assert_eq!(undo.len(), 1);

    let redo = apply_undo(&mut world, &undo.pop().unwrap());
    let camera = world.query_one_mut::<&Camera>(entity).unwrap();
    assert!((camera.fov - 60.0f32.to_radians()).abs() < 1e-4);
    assert_eq!(camera.speed, 8.0);
    match redo {
        EditorCommand::Camera { old_camera, .. } => {
            assert!((old_camera.fov - 90.0f32.to_radians()).abs() < 1e-4);
            assert_eq!(old_camera.speed, 16.0);
        }
        _ => panic!("expected Camera undo command"),
    }
}

fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("need device")
        .0
}

#[test]
fn extract_prefers_mesh_over_light() {
    let device = headless_device();
    let mut world = World::new();
    let cpu_mesh = aether_engine::asset::mesh::CpuMesh::cube();
    let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
        &device, &cpu_mesh,
    ));
    let entity = world.spawn((
        Transform::default(),
        aether_engine::ecs::components::MeshHandle::new(
            gpu_mesh,
            aether_engine::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            unlit: 0,
            _pad: 0,
            albedo_texture_id: 0,
            ..MaterialUniform::default()
        },
        Selected,
    ));
    let target = extract(&world).expect("should extract mesh target");
    assert_eq!(target.entity(), entity);
    assert!(matches!(target, InspectorTarget::Mesh { .. }));
}

#[test]
fn apply_material_undo_restores_source_config_and_adapter() {
    let device = headless_device();
    let cpu_mesh = aether_engine::asset::mesh::CpuMesh::cube();
    let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
        &device, &cpu_mesh,
    ));
    let initial = MaterialConfig::default();
    let mut world = World::new();
    let entity = world.spawn((
        Transform::default(),
        aether_engine::ecs::components::MeshHandle::new(
            gpu_mesh,
            aether_engine::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        initial.clone(),
        MaterialUniform::default(),
        Selected,
    ));
    let mut target = extract(&world).expect("mesh target should exist");
    if let InspectorTarget::Mesh { material, .. } = &mut target {
        material.emissive = [0.2, 0.4, 0.8];
        material.emissive_intensity = 3.0;
    } else {
        panic!("expected mesh target");
    }

    let mut undo = Vec::new();
    let mut redo = Vec::new();
    apply(
        &target,
        &mut world,
        &mut undo,
        &mut redo,
        &mut aether_engine::asset::AssetManager::new(),
    )
    .unwrap();
    assert_eq!(
        world
            .query_one::<&MaterialConfig>(entity)
            .get()
            .unwrap()
            .emissive,
        [0.2, 0.4, 0.8]
    );
    assert_eq!(undo.len(), 1);

    let redo_command = apply_undo(&mut world, &undo.pop().unwrap());
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &initial
    );
    assert_eq!(
        *world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        MaterialUniform::default()
    );
    assert!(matches!(redo_command, EditorCommand::Material { .. }));
}
