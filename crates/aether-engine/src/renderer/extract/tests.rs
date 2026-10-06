use super::*;
use crate::asset::mesh::GpuMesh;
use crate::asset::registry::BuiltinMeshRegistry;
use crate::asset::terrain_material::TerrainMaterial;
use crate::ecs::components::{Atmosphere, Clouds, GodRay, Terrain, Water};
use crate::ecs::components::{Name, Visibility};
use crate::ecs::World;
use crate::math::{Frustum, Vec3};
use crate::renderer::extracted_material::ExtractedMaterial;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::config::material::{OrmSwizzle, TextureChannel};
use crate::scene::material::MaterialResolver;
use crate::scene::{
    AtmosphereConfig, CloudConfig, GodRayConfig, MaterialConfig, TerrainGeometry, TerrainSource,
    WaterConfig,
};

fn default_terrain() -> Terrain {
    Terrain {
        source: TerrainSource::Procedural {
            seed: 0,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: TerrainGeometry::default(),
        material: TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: Vec::new(),
    }
}
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

#[test]
fn extract_without_culling_includes_all_visible() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        MeshHandle::new(
            cube_gpu.clone(),
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name("visible".into()),
    ));

    let batches = extract_render_batches(&world);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].instances.len(), 1);
}

#[test]
fn extract_with_culling_skips_outside_entities() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

    let mut world = World::new();
    // Inside the identity NDC cube.
    world.spawn((
        Transform::default(),
        MeshHandle::new(
            cube_gpu.clone(),
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name("inside".into()),
    ));
    // Far outside the identity NDC cube.
    world.spawn((
        Transform {
            translation: Vec3::new(10.0, 0.0, 0.0),
            ..Default::default()
        },
        MeshHandle::new(
            cube_gpu.clone(),
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform::default(),
        Visibility::default(),
        Name("outside".into()),
    ));

    let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
    let batches = extract_render_batches_with_frustum_culling(&world, Some(&frustum));
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].instances.len(), 1);
}

#[test]
fn extract_respects_visibility_component() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        MeshHandle::new(
            cube_gpu.clone(),
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        MaterialUniform::default(),
        Visibility(false),
        Name("hidden".into()),
    ));

    let batches = extract_render_batches(&world);
    assert!(batches.is_empty());
}

#[test]
fn extract_does_not_instance_batch_different_extended_materials() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));

    let normal_mapped = MaterialUniform {
        normal_scale: 0.5,
        normal_texture_id: 11,
        ..MaterialUniform::default()
    };

    let mut world = World::new();
    for (name, material) in [
        ("plain", MaterialUniform::default()),
        ("normal", normal_mapped),
    ] {
        world.spawn((
            Transform::default(),
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            material,
            Visibility::default(),
            Name(name.into()),
        ));
    }

    let batches = extract_render_batches(&world);
    assert_eq!(batches.len(), 2);
}

#[test]
fn opaque_instancing_toggle_preserves_entities_and_stable_order() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));
    let mut world = World::new();
    for index in (0..4).rev() {
        world.spawn((
            Transform {
                translation: Vec3::new(index as f32, 0.0, 0.0),
                ..Default::default()
            },
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform::default(),
            Visibility::default(),
            Name(format!("instancing-{index}")),
        ));
    }

    let instanced = extract_render_batches_with_instancing(&world, None, true);
    let individual = extract_render_batches_with_instancing(&world, None, false);
    assert_eq!(instanced.len(), 1);
    assert_eq!(instanced[0].instances.len(), 4);
    assert_eq!(individual.len(), 4);
    assert!(individual
        .windows(2)
        .all(|pair| pair[0].instances[0].entity_id < pair[1].instances[0].entity_id));
    let mut instanced_ids = instanced[0]
        .instances
        .iter()
        .map(|instance| instance.entity_id)
        .collect::<Vec<_>>();
    let individual_ids = individual
        .iter()
        .map(|batch| batch.instances[0].entity_id)
        .collect::<Vec<_>>();
    instanced_ids.sort_unstable();
    assert_eq!(instanced_ids, individual_ids);
}

#[test]
fn material_extraction_switch_uses_resolved_component_and_legacy_adapter() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));
    let legacy = MaterialUniform {
        albedo: [0.9, 0.1, 0.1, 1.0],
        ..MaterialUniform::default()
    };
    let config = MaterialConfig {
        albedo: [0.1, 0.7, 0.3, 1.0],
        orm_swizzle: OrmSwizzle {
            ao: TextureChannel::A,
            roughness: TextureChannel::Zero,
            metallic: TextureChannel::One,
        },
        ..MaterialConfig::default()
    };
    let resolution = MaterialResolver::new(".")
        .resolve(&config, &mut crate::asset::AssetManager::new())
        .unwrap();
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        MeshHandle::new(
            cube_gpu,
            crate::ecs::components::MeshSource::Builtin("cube".into()),
            "cube",
        ),
        legacy,
        ExtractedMaterial::from_resolution(&resolution),
        Visibility::default(),
        Name("resolved material wins".into()),
    ));

    let active = extract_render_batches_with_material_mode(&world, None, true, true);
    assert_eq!(active.len(), 1);
    assert_eq!(
        active[0].extracted_material.uniform_adapter().albedo,
        config.albedo
    );
    assert_eq!(active[0].extracted_material.orm_swizzle_bits(), [3, 4, 5]);
    let object = crate::renderer::renderable::ObjectUniform::from_extracted_material(
        &active[0].extracted_material,
    );
    assert_eq!(object.orm_swizzle, 3 | (4 << 3) | (5 << 6));

    let compatibility = extract_render_batches_with_material_mode(&world, None, true, false);
    assert_eq!(compatibility.len(), 1);
    assert_eq!(
        compatibility[0].extracted_material.uniform_adapter().albedo,
        legacy.albedo
    );
    assert_eq!(
        compatibility[0].extracted_material.orm_swizzle_bits(),
        [0, 1, 2]
    );
}

#[test]
fn material_batch_identity_includes_orm_channel_mapping() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let cube_cpu = registry.get("cube").unwrap();
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&device, &cube_cpu));
    let mut world = World::new();
    for (index, swizzle) in [
        OrmSwizzle::default(),
        OrmSwizzle {
            ao: TextureChannel::A,
            roughness: TextureChannel::G,
            metallic: TextureChannel::B,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let resolution = MaterialResolver::new(".")
            .resolve(
                &MaterialConfig {
                    orm_swizzle: swizzle,
                    ..MaterialConfig::default()
                },
                &mut crate::asset::AssetManager::new(),
            )
            .unwrap();
        world.spawn((
            Transform::default(),
            MeshHandle::new(
                cube_gpu.clone(),
                crate::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            ExtractedMaterial::from_resolution(&resolution),
            Visibility::default(),
            Name(format!("orm-{index}")),
        ));
    }
    let batches = extract_render_batches_with_material_mode(&world, None, true, true);
    assert_eq!(batches.len(), 2);
}

#[test]
fn extract_optional_data_is_empty_by_default() {
    let world = World::new();
    let optional = extract_optional_pass_data(&world);
    assert_eq!(optional, OptionalPassData::default());
}

#[test]
fn extract_optional_data_finds_all_components() {
    let mut world = World::new();
    world.spawn((default_terrain(),));
    world.spawn((Water {
        config: WaterConfig::default(),
        dudv_texture: None,
        normal_texture: None,
    },));
    world.spawn((Atmosphere {
        config: AtmosphereConfig::default(),
    },));
    world.spawn((Clouds {
        config: CloudConfig::default(),
    },));
    world.spawn((GodRay {
        config: GodRayConfig::default(),
    },));

    let optional = extract_optional_pass_data(&world);
    assert!(optional.terrain.is_some());
    assert!(optional.water.is_some());
    assert!(optional.atmosphere.is_some());
    assert!(optional.clouds.is_some());
    assert!(optional.god_ray.is_some());
}
