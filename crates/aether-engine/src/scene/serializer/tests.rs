use super::*;
use crate::asset::registry::BuiltinMeshRegistry;
use crate::ecs::components::{Light, Name, Transform, Visibility};
use crate::ecs::World;
use crate::renderer::light::LightType;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    AtmosphereConfig, CameraConfig, LightConfig, MeshRef, ObjectConfig, TerrainGeometry,
    TerrainSource,
};
use glam::{Quat, Vec3};
use std::sync::Arc;

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

fn spawn_camera_entity(world: &mut World, pos: [f32; 3], yaw: f32, pitch: f32, fov: f32) {
    world.spawn((
        Transform {
            translation: Vec3::from_array(pos),
            rotation: Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0),
            scale: Vec3::ONE,
        },
        crate::ecs::components::Camera {
            fov: fov.to_radians(),
            ..Default::default()
        },
    ));
}

fn spawn_light_entity(world: &mut World, color: [f32; 3], intensity: f32) {
    world.spawn((
        Transform {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        Light {
            light_type: LightType::Directional,
            color,
            intensity,
            cast_shadow: true,
        },
    ));
}

fn spawn_object_entity(
    world: &mut World,
    device: &wgpu::Device,
    registry: &BuiltinMeshRegistry,
    name: &str,
    mesh_name: &str,
) {
    let cpu_mesh = registry.get(mesh_name).expect("known mesh");
    let gpu_mesh = Arc::new(crate::asset::mesh::GpuMesh::from_cpu(device, &cpu_mesh));
    world.spawn((
        Transform::default(),
        crate::ecs::components::MeshHandle::new(
            gpu_mesh,
            crate::ecs::components::MeshSource::Builtin(mesh_name.into()),
            mesh_name,
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name(name.into()),
    ));
}

#[test]
fn extract_camera_roundtrips() {
    let mut world = World::new();
    spawn_camera_entity(&mut world, [5.0, 10.0, 5.0], -1.5, -0.5, 60.0);

    let camera = extract_camera(&world);
    assert_eq!(camera.position, [5.0, 10.0, 5.0]);
    assert!((camera.yaw - (-1.5)).abs() < 0.001);
    assert!((camera.pitch - (-0.5)).abs() < 0.001);
    assert!((camera.fov - 60.0).abs() < 0.001);
}

#[test]
fn extract_camera_defaults_when_missing() {
    let world = World::new();
    let camera = extract_camera(&world);
    assert_eq!(camera, CameraConfig::default());
}

#[test]
fn extract_light_roundtrips() {
    let mut world = World::new();
    spawn_light_entity(&mut world, [1.0, 0.9, 0.8], 2.0);

    let lights = extract_lights(&world);
    assert_eq!(lights.len(), 1);
    let light = &lights[0];
    assert_eq!(light.light_type, LightType::Directional);
    assert_eq!(light.color, [1.0, 0.9, 0.8]);
    assert_eq!(light.intensity, 2.0);
    // Direction should be approximately -Y
    let dir = Vec3::from_array(light.direction);
    assert!((dir - Vec3::NEG_Y).length() < 0.001);
}

#[test]
fn extract_light_empty_when_no_lights() {
    let world = World::new();
    let lights = extract_lights(&world);
    assert!(lights.is_empty());
}

#[test]
fn extract_object_preserves_name() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let mut world = World::new();
    spawn_object_entity(&mut world, &device, &registry, "MyCube", "cube");

    let objects = extract_objects(&world);
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "MyCube");
    assert_eq!(objects[0].mesh, MeshRef::Builtin("cube".into()));
}

#[test]
fn extract_object_preserves_file_mesh_source() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let mut world = World::new();
    let cpu_mesh = registry.get("cube").expect("known mesh");
    let gpu_mesh = Arc::new(crate::asset::mesh::GpuMesh::from_cpu(&device, &cpu_mesh));
    world.spawn((
        Transform::default(),
        crate::ecs::components::MeshHandle::new(
            gpu_mesh,
            crate::ecs::components::MeshSource::File("assets/models/dragon.obj".into()),
            "assets/models/dragon.obj",
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name("Dragon".into()),
    ));

    let objects = extract_objects(&world);
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "Dragon");
    assert_eq!(
        objects[0].mesh,
        MeshRef::File("assets/models/dragon.obj".into())
    );
}

#[test]
fn serialize_world_full_roundtrip() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let mut world = World::new();

    spawn_camera_entity(&mut world, [1.0, 2.0, 3.0], -1.0, -0.5, 60.0);
    spawn_light_entity(&mut world, [1.0, 1.0, 1.0], 1.5);
    spawn_object_entity(&mut world, &device, &registry, "TestCube", "cube");

    let lighting = LightingUniforms::default();
    let desc = serialize_world(&world, &lighting, "TestScene");

    assert_eq!(desc.name, "TestScene");
    assert_eq!(desc.camera.position, [1.0, 2.0, 3.0]);
    assert_eq!(desc.lights.len(), 1);
    assert_eq!(desc.lights[0].intensity, 1.5);
    assert_eq!(desc.objects.len(), 1);
    assert_eq!(desc.objects[0].name, "TestCube");
    assert_eq!(desc.ambient, lighting.ambient_intensity);
}

#[test]
fn extract_object_spawned_with_selected_directly() {
    use crate::ecs::components::Selected;
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let mut world = World::new();
    let cpu_mesh = registry.get("cube").expect("known mesh");
    let gpu_mesh = Arc::new(crate::asset::mesh::GpuMesh::from_cpu(&device, &cpu_mesh));
    world.spawn((
        Transform::default(),
        crate::ecs::components::MeshHandle::new(
            gpu_mesh,
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name("DefaultCube".into()),
        Selected,
    ));

    let objects = extract_objects(&world);
    assert_eq!(
        objects.len(),
        1,
        "object spawned with 6 components directly should be extracted"
    );
    assert_eq!(objects[0].name, "DefaultCube");
}

#[test]
fn serialize_to_ron_roundtrips() {
    let desc = SceneDescription {
        name: "Roundtrip".into(),
        camera: CameraConfig {
            position: [1.0, 2.0, 3.0],
            yaw: -1.0,
            pitch: -0.5,
            speed: 4.0,
            fov: 60.0,
            near: 0.1,
            far: 1000.0,
        },
        lights: vec![LightConfig {
            light_type: LightType::Directional,
            direction: [0.0, -1.0, 0.0],
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        }],
        ambient: 0.05,
        terrain: None,
        atmosphere: None,
        water: None,
        clouds: None,
        god_ray: None,
        objects: vec![ObjectConfig {
            name: "Cube".into(),
            mesh: MeshRef::Builtin("cube".into()),
            transform: TransformConfig::default(),
            material: MaterialConfig::default(),
        }],
    };

    let ron = to_ron_string(&desc).expect("should serialize");
    let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
    assert_eq!(parsed, desc);
}

#[test]
fn serialize_world_preserves_terrain() {
    use crate::ecs::components::Terrain;
    let mut world = World::new();
    world.spawn((Terrain {
        source: TerrainSource::Procedural {
            seed: 99,
            frequency: 0.1,
            amplitude: 32.0,
        },
        geometry: TerrainGeometry {
            extent: 256.0,
            chunk_size: 64,
            max_lod: 4,
            albedo_tiling: 64.0,
        },
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![
            crate::scene::TerrainLayerConfig::default(),
            crate::scene::TerrainLayerConfig::default(),
            crate::scene::TerrainLayerConfig::default(),
            crate::scene::TerrainLayerConfig::default(),
        ],
    },));

    let lighting = LightingUniforms::default();
    let desc = serialize_world(&world, &lighting, "TerrainScene");

    let terrain = desc.terrain.expect("terrain should be serialized");
    assert_eq!(
        terrain.source,
        TerrainSource::Procedural {
            seed: 99,
            frequency: 0.1,
            amplitude: 32.0,
        }
    );
    assert_eq!(terrain.geometry.extent, 256.0);
    assert_eq!(terrain.geometry.chunk_size, 64);
    assert_eq!(terrain.geometry.max_lod, 4);
}

#[test]
fn serialize_world_preserves_atmosphere() {
    use crate::ecs::components::Atmosphere;
    let mut world = World::new();
    world.spawn((Atmosphere {
        config: AtmosphereConfig {
            sun_direction: [0.0, 0.1, -1.0],
            ..Default::default()
        },
    },));

    let lighting = LightingUniforms::default();
    let desc = serialize_world(&world, &lighting, "AtmosphereScene");

    let atmos = desc.atmosphere.expect("atmosphere should be serialized");
    assert_eq!(atmos.sun_direction, [0.0, 0.1, -1.0]);
}

#[test]
fn serialize_world_preserves_water() {
    use crate::ecs::components::Water;
    let mut world = World::new();
    world.spawn((Water {
        config: WaterConfig {
            level: -0.5,
            wave_amplitude: 0.5,
            ..Default::default()
        },
        dudv_texture: None,
        normal_texture: None,
    },));

    let lighting = LightingUniforms::default();
    let desc = serialize_world(&world, &lighting, "WaterScene");

    let water = desc.water.expect("water should be serialized");
    assert_eq!(water.level, -0.5);
    assert_eq!(water.wave_amplitude, 0.5);
}
