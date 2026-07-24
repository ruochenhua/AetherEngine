use super::*;
use crate::test_utils::headless_device_queue;
use crate::ecs::components::Terrain;
use crate::ecs::World;
use crate::math::{Frustum, Mat4, Vec3};
use crate::renderer::extract::extract_optional_pass_data;
use crate::renderer::frame::{FrameConfig, RenderFrame};
use crate::scene::{TerrainGeometry as TerrainGeometryConfig, TerrainSource};
use crate::terrain::{Chunk, TerrainGeometry};

fn init_ctx<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> InitContext<'a> {
    let texture_cache = Box::leak(Box::new(crate::asset::texture_cache::GpuTextureCache::new(
        device, queue,
    )));
    InitContext {
        device,
        queue,
        surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
        depth_format: wgpu::TextureFormat::Depth32Float,
        width: 64,
        height: 64,
        ibl_resources: None,
        texture_cache,
    }
}

fn default_terrain() -> Terrain {
    Terrain {
        source: TerrainSource::Procedural {
            seed: 1,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: TerrainGeometryConfig {
            extent: 128.0,
            chunk_size: 64,
            max_lod: 2,
            albedo_tiling: 64.0,
        },
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: vec![],
    }
}

fn build_terrain_geometry(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    terrain: &Terrain,
) -> TerrainGeometry {
    let mut geom = TerrainGeometry::new(device);
    let camera = crate::renderer::camera::FlyCamera::default();
    geom.update(device, queue, &camera, 1.0, terrain);
    geom
}

#[test]
fn terrain_pass_signature_declares_gbuffer_outputs() {
    let Some((device, queue)) = headless_device_queue() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let ctx = init_ctx(&device, &queue);
    let pass = TerrainPass::init(&ctx);
    let sig = pass.signature();
    assert_eq!(sig.writes.len(), 5);
}

#[test]
fn terrain_pass_skipped_when_no_terrain_component() {
    let Some((device, queue)) = headless_device_queue() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let ctx = init_ctx(&device, &queue);
    let pass = TerrainPass::init(&ctx);
    let world = World::new();
    let optional = extract_optional_pass_data(&world);
    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = crate::renderer::light::LightingUniforms::default();
    let assets = crate::asset::AssetManager::new();
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        config: &FrameConfig::default(),
        optional: &optional,
        terrain_geometry: None,
        texture_cache: ctx.texture_cache,
        asset_manager: &assets,
    };
    assert!(!pass.should_run(&frame));
}

#[test]
fn terrain_pass_runs_when_terrain_component_present() {
    let Some((device, queue)) = headless_device_queue() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let ctx = init_ctx(&device, &queue);
    let mut pass = TerrainPass::init(&ctx);
    let mut world = World::new();
    let terrain = default_terrain();
    world.spawn((terrain.clone(),));
    let optional = extract_optional_pass_data(&world);
    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = crate::renderer::light::LightingUniforms::default();
    let assets = crate::asset::AssetManager::new();
    let geom = build_terrain_geometry(&device, &queue, &terrain);
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        config: &FrameConfig::default(),
        optional: &optional,
        terrain_geometry: Some(std::sync::Arc::new(std::sync::RwLock::new(geom))),
        texture_cache: ctx.texture_cache,
        asset_manager: &assets,
    };
    pass.apply_frame(&frame);
    assert!(pass.should_run(&frame));
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
