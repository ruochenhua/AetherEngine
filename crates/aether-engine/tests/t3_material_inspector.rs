use aether_engine::asset::mesh::GpuMesh;
use aether_engine::asset::registry::BuiltinMeshRegistry;
use aether_engine::ecs::components::{MeshHandle, MeshSource, Name, Transform, Visibility};
use aether_engine::ecs::World;
use aether_engine::renderer::light::LightingUniforms;
use aether_engine::renderer::renderable::MaterialUniform;
use aether_engine::scene::serializer::{serialize_world, to_ron_string};
use aether_engine::scene::{MaterialConfig, SceneDescription};
use std::sync::Arc;

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
fn serialize_world_roundtrips_extended_material_config() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cpu_mesh = registry.get("cube").expect("known mesh");
    let gpu_mesh = Arc::new(GpuMesh::from_cpu(&device, &cpu_mesh));
    let config = MaterialConfig {
        normal_texture: Some("textures/detail_normal.png".into()),
        orm_texture: Some("textures/detail_orm.png".into()),
        emissive_texture: Some("textures/glow.png".into()),
        normal_scale: 1.4,
        occlusion_strength: 0.65,
        emissive: [0.1, 0.3, 0.8],
        emissive_intensity: 2.5,
        ..MaterialConfig::default()
    };
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
        config.clone(),
        MaterialUniform::default(),
        Visibility::default(),
        Name("ExtendedMaterial".into()),
    ));

    let desc = serialize_world(&world, &LightingUniforms::default(), "MaterialScene");
    let roundtrip = SceneDescription::from_ron(&to_ron_string(&desc).unwrap()).unwrap();
    assert_eq!(roundtrip.objects[0].material, config);
}
