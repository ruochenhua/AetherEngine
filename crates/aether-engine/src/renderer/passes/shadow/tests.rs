use super::*;
use crate::renderer::camera::FlyCamera;
use crate::renderer::frame::{FrameConfig, RenderFrame};
use crate::renderer::light::LightingUniforms;

fn headless_queue() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("need device")
}

fn build_frame<'a>(
    camera: &'a FlyCamera,
    lighting: &'a LightingUniforms,
    queue: &'a wgpu::Queue,
    optional: &'a crate::renderer::extract::OptionalPassData,
    config: &'a FrameConfig,
    texture_cache: &'a crate::asset::texture_cache::GpuTextureCache,
    asset_manager: &'a crate::asset::AssetManager,
) -> RenderFrame<'a> {
    RenderFrame {
        camera,
        aspect: 16.0 / 9.0,
        batches: std::sync::Arc::from([]),
        lighting,
        queue,
        delta_time: 0.0,
        config,
        optional,
        terrain_geometry: None,
        texture_cache,
        asset_manager,
    }
}

#[test]
fn compute_cascades_produces_valid_matrices() {
    let camera = FlyCamera::default();
    let lighting = LightingUniforms::default();
    let (device, queue) = headless_queue();
    let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
    let asset_manager = crate::asset::AssetManager::new();
    let optional = crate::renderer::extract::OptionalPassData::default();
    let config = FrameConfig::default();
    let frame = build_frame(
        &camera,
        &lighting,
        &queue,
        &optional,
        &config,
        &texture_cache,
        &asset_manager,
    );
    let light_dir = glam::Vec3::new(0.5, -1.0, 0.3).normalize();
    let cascades = compute_cascades(&frame, &light_dir);
    for cascade in &cascades {
        for c in 0..4 {
            for r in 0..4 {
                assert!(!cascade.view_proj.col(c)[r].is_nan());
            }
        }
        assert!(cascade.split_depth > 0.0);
    }
    assert!(cascades[0].split_depth <= cascades[1].split_depth);
    assert!(cascades[1].split_depth <= cascades[2].split_depth);
    drop(device);
}

#[test]
fn split_distances_increase_monotonically() {
    let splits = split_distances(0.1, 100.0, 3, 0.5);
    assert_eq!(splits.len(), 3);
    assert!(splits[0] < splits[1]);
    assert!(splits[1] < splits[2]);
    assert!(splits[2] <= 100.0);
}

#[test]
fn cascade_matrix_contains_scene_points() {
    let camera = FlyCamera::default();
    let lighting = LightingUniforms::default();
    let (device, queue) = headless_queue();
    let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
    let asset_manager = crate::asset::AssetManager::new();
    let optional = crate::renderer::extract::OptionalPassData::default();
    let config = FrameConfig::default();
    let frame = build_frame(
        &camera,
        &lighting,
        &queue,
        &optional,
        &config,
        &texture_cache,
        &asset_manager,
    );
    let light_dir = glam::Vec3::new(-0.6, -1.0, -0.4).normalize();
    let cascades = compute_cascades(&frame, &light_dir);

    // Test that a few world-space points map inside the first cascade's clip space.
    let test_points = [
        glam::Vec3::new(0.0, 0.0, 0.0),
        glam::Vec3::new(2.0, 0.0, 4.0),
        glam::Vec3::new(-8.0, 1.5, -8.0),
    ];
    for p in test_points {
        let clip = cascades[0].view_proj * glam::Vec4::from((p, 1.0));
        let ndc = clip.xyz() / clip.w;
        assert!(
            ndc.x >= -1.0
                && ndc.x <= 1.0
                && ndc.y >= -1.0
                && ndc.y <= 1.0
                && ndc.z >= 0.0
                && ndc.z <= 1.0,
            "point {:?} outside cascade 0 clip space: {:?}",
            p,
            ndc
        );
    }
    drop(device);
}

#[test]
fn shadow_pass_stores_terrain_geometry() {
    let camera = FlyCamera::default();
    let lighting = LightingUniforms::default();
    let (device, queue) = headless_queue();
    let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
    let asset_manager = crate::asset::AssetManager::new();
    let optional = crate::renderer::extract::OptionalPassData::default();
    let config = FrameConfig::default();
    let mut terrain_geom = crate::terrain::TerrainGeometry::new(&device);
    let terrain = crate::ecs::components::Terrain {
        source: crate::scene::TerrainSource::Procedural {
            seed: 0,
            frequency: 0.05,
            amplitude: 32.0,
        },
        geometry: crate::scene::TerrainGeometry::default(),
        material: crate::asset::terrain_material::TerrainMaterial::default(),
        splatmap_path: None,
        layer_configs: Vec::new(),
    };
    terrain_geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain);

    let frame = RenderFrame {
        camera: &camera,
        aspect: 16.0 / 9.0,
        batches: std::sync::Arc::from([]),
        lighting: &lighting,
        queue: &queue,
        delta_time: 0.0,
        config: &config,
        optional: &optional,
        terrain_geometry: Some(std::sync::Arc::new(std::sync::RwLock::new(terrain_geom))),
        texture_cache: &texture_cache,
        asset_manager: &asset_manager,
    };

    let mut pass = ShadowPass::new(&device);
    pass.apply_frame(&frame);
    assert!(pass.terrain_geometry.is_some());
    assert!(!pass
        .terrain_geometry
        .unwrap()
        .read()
        .unwrap()
        .chunks()
        .is_empty());
    drop(device);
}

#[test]
fn shadow_shader_filters_edge_on_triangles() {
    assert!(shaders::SHADOW_SHADER_SRC.contains("dpdx(in.world_position)"));
    assert!(shaders::SHADOW_SHADER_SRC.contains("dpdy(in.world_position)"));
    assert!(shaders::SHADOW_SHADER_SRC.contains("discard;"));
}
