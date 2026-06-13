use super::*;
use crate::ecs::components::Terrain;
use crate::ecs::World;
use crate::math::{Frustum, Mat4, Vec3};
use crate::renderer::frame::RenderFrame;
use crate::scene::{TerrainGeometry, TerrainSource};
use crate::terrain::Chunk;

fn headless_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("need device")
}

#[test]
fn terrain_pass_signature_declares_gbuffer_outputs() {
    let (device, _queue) = headless_device();
    let pass = TerrainPass::init(&device);
    let sig = pass.signature();
    assert_eq!(sig.writes.len(), 5);
}

#[test]
fn terrain_pass_skipped_when_no_terrain_component() {
    let (device, queue) = headless_device();
    let pass = TerrainPass::init(&device);
    let world = World::new();
    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = crate::renderer::light::LightingUniforms::default();
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        world: &world,
    };
    assert!(!pass.should_run(&frame));
}

#[test]
fn terrain_pass_runs_when_terrain_component_present() {
    let (device, queue) = headless_device();
    let mut pass = TerrainPass::init(&device);
    let mut world = World::new();
    world.spawn((Terrain {
        source: TerrainSource::Procedural {
            seed: 1,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: TerrainGeometry {
            extent: 128.0,
            chunk_size: 64,
            max_lod: 2,
        },
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![],
    },));
    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = crate::renderer::light::LightingUniforms::default();
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        world: &world,
    };
    pass.apply_frame(&frame);
    assert!(pass.should_run(&frame));
}

#[test]
fn terrain_pass_rebuilds_chunks_when_config_changes() {
    let (device, queue) = headless_device();
    let mut pass = TerrainPass::init(&device);
    let mut world = World::new();
    let entity = world.spawn((Terrain {
        source: TerrainSource::Procedural {
            seed: 1,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: TerrainGeometry {
            extent: 128.0,
            chunk_size: 64,
            max_lod: 2,
        },
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![],
    },));
    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = crate::renderer::light::LightingUniforms::default();
    let first_chunk_count = {
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            world: &world,
        };
        pass.apply_frame(&frame);
        pass.chunks.len()
    };
    assert!(first_chunk_count > 0);

    // Change extent: geometry config changes.
    let _ = world.despawn(entity);
    world.spawn((Terrain {
        source: TerrainSource::Procedural {
            seed: 1,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: TerrainGeometry {
            extent: 256.0,
            chunk_size: 64,
            max_lod: 2,
        },
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![],
    },));
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        world: &world,
    };
    pass.apply_frame(&frame);
    assert!(
        pass.chunks.len() > first_chunk_count,
        "expected more chunks after doubling extent, got {} vs {}",
        pass.chunks.len(),
        first_chunk_count
    );
}

#[test]
fn terrain_chunk_aabb_inside_frustum_is_visible() {
    // Place a small chunk fully inside a small orthographic frustum.
    let chunk = Chunk::new(0, 0, Vec3::new(0.0, 0.0, 0.0), 1.0, 2);
    let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 0.1, 10.0);
    let frustum = Frustum::from_view_projection(proj * view);
    let aabb = chunk.aabb(-1.0, 1.0);
    assert_eq!(
        aabb.intersects_frustum(&frustum),
        crate::math::CullingVisibility::Visible
    );
}

#[test]
fn terrain_chunk_lod_selects_by_distance() {
    let mut chunk = Chunk::new(0, 0, Vec3::ZERO, 64.0, 4);
    chunk.select_lod(Vec3::ZERO, 2.0);
    assert_eq!(chunk.lod, 0);
    chunk.select_lod(Vec3::new(1000.0, 0.0, 0.0), 2.0);
    assert!(chunk.lod > 0);
}
