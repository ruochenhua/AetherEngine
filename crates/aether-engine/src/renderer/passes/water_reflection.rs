//! Planar water reflection pass.
//
// Renders a mirror image of opaque meshes into a texture that the water pass
// samples for reflections. The camera is mirrored across the water plane
// (Y = level) and the scene is drawn with a simple forward lit shader.
//
// The pass is always present in the pipeline but skips execution when the
// scene has no water or when `reflection_enabled` is false.

use crate::asset::texture::GpuTexture;
use crate::renderer::extract::RenderBatch;
use crate::renderer::pass::ResHandle;
use crate::renderer::resource::{WaterReflectionColor, WaterReflectionDepth};
use crate::terrain::TerrainGeometry;
use glam::{Mat4, Vec3};
use std::sync::{Arc, RwLock};

mod pass;
mod pipeline;
mod shaders;
mod terrain;

/// Planar reflection pass state.
pub struct WaterReflectionPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    object_buffer: wgpu::Buffer,
    object_buffer_capacity: usize,
    object_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,

    terrain_geometry: Option<Arc<RwLock<TerrainGeometry>>>,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_buffer: wgpu::Buffer,
    terrain_bind_group: wgpu::BindGroup,
    terrain_bind_group_layout: wgpu::BindGroupLayout,
    terrain_last_splat: Option<Arc<GpuTexture>>,
    terrain_last_layer0: Option<Arc<GpuTexture>>,
    terrain_last_layer1: Option<Arc<GpuTexture>>,
    terrain_last_layer2: Option<Arc<GpuTexture>>,
    terrain_last_layer3: Option<Arc<GpuTexture>>,

    color_handle: Option<ResHandle<WaterReflectionColor>>,
    depth_handle: Option<ResHandle<WaterReflectionDepth>>,

    batches: Arc<[RenderBatch]>,
    view: Mat4,
    proj: Mat4,
    light_dir: Vec3,
    light_color: Vec3,
    ambient: Vec3,
    has_water: bool,
    reflection_enabled: bool,
    reflection_level: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ReflectionUniform {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::{FrameConfig, RenderFrame};
    use crate::renderer::light::LightingUniforms;
    use crate::renderer::pass::Pass;
    use crate::scene::{TerrainGeometry as TerrainGeometryConfig, TerrainSource, WaterConfig};

    fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn water_reflection_pass_stores_terrain_geometry() {
        let (device, queue) = headless_device();
        let pass = WaterReflectionPass::new(&device, &queue);
        assert!(pass.terrain_geometry.is_none());

        let mut world = World::new();
        world.spawn((crate::ecs::components::Water {
            config: WaterConfig {
                reflection_enabled: true,
                ..Default::default()
            },
            dudv_texture: None,
            normal_texture: None,
        },));
        let terrain = crate::ecs::components::Terrain {
            source: TerrainSource::Procedural {
                seed: 0,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: TerrainGeometryConfig::default(),
            material: crate::asset::terrain_material::TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: Vec::new(),
        };
        world.spawn((terrain.clone(),));
        let optional = extract_optional_pass_data(&world);
        let camera = crate::renderer::camera::FlyCamera::default();
        let lighting = LightingUniforms::default();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();

        let mut terrain_geom = TerrainGeometry::new(&device);
        terrain_geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain);

        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 16.0 / 9.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
            terrain_geometry: Some(std::sync::Arc::new(std::sync::RwLock::new(terrain_geom))),
            texture_cache: &texture_cache,
            asset_manager: &asset_manager,
        };

        let mut pass = WaterReflectionPass::new(&device, &queue);
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
        assert!(pass.terrain_geometry.is_some());
    }
}
