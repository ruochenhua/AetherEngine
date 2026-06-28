//! Water Pass — transparent forward water surface with Gerstner waves.
//!
//! Renders a large subdivided plane displaced by Gerstner waves on the GPU.
//! The pass runs after SSR and before composite. It samples the lit scene
//! color for refraction and the SSR reflection texture for reflections, then
//! blends the result into a separate `WaterColor` overlay that the composite
//! pass mixes over the opaque scene.

use crate::asset::mesh::GpuMesh;
use crate::asset::texture::GpuTexture;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{
    GDepth, ReflectionTexture, SceneColor, WaterColor, WaterReflectionColor,
};
use crate::renderer::resource_table::ResourceTable;
use std::sync::Arc;

mod execute;
mod pipeline;
mod types;

pub use types::WaterUniform;

/// Water render pass.
pub struct WaterPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group: Option<wgpu::BindGroup>,
    water_texture_bind_group: Option<wgpu::BindGroup>,
    water_texture_bind_group_layout: wgpu::BindGroupLayout,
    water_sampler: wgpu::Sampler,
    scene_sampler: wgpu::Sampler,
    fallback_dudv: Arc<GpuTexture>,
    fallback_normal: Arc<GpuTexture>,
    mesh: Arc<GpuMesh>,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    reflection_handle: Option<ResHandle<ReflectionTexture>>,
    planar_reflection_handle: Option<ResHandle<WaterReflectionColor>>,
    depth_handle: Option<ResHandle<GDepth>>,
    water_color_handle: Option<ResHandle<WaterColor>>,
    has_water: bool,
    time: f32,
    /// Last resolved dudv texture; kept alive so the cached bind group stays valid.
    last_dudv: Option<Arc<crate::asset::texture::GpuTexture>>,
    /// Last resolved normal texture; kept alive so the cached bind group stays valid.
    last_normal: Option<Arc<crate::asset::texture::GpuTexture>>,
}

impl Pass for WaterPass {
    fn name(&self) -> &str {
        "Water"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Water")
            .read::<SceneColor>()
            .read::<ReflectionTexture>()
            .read::<WaterReflectionColor>()
            .read::<GDepth>()
            .write::<WaterColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.queue)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>());
        self.reflection_handle = Some(resources.handle::<ReflectionTexture>());
        self.planar_reflection_handle = Some(resources.handle::<WaterReflectionColor>());
        self.depth_handle = Some(resources.handle::<GDepth>());
        self.water_color_handle = Some(resources.handle::<WaterColor>());

        let texture_bind_group_layout = pipeline::create_texture_bind_group_layout(device);

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.scene_color_handle.unwrap()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.reflection_handle.unwrap()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.scene_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.depth_handle.unwrap()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.planar_reflection_handle.unwrap()),
                    ),
                },
            ],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_water
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(water) = frame.optional.water.clone() {
            self.has_water = true;
            self.time += frame.delta_time;

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let view_proj = proj * view;
            let inv_view_proj = view_proj.inverse();

            let cfg = water.config;
            let uniforms = WaterUniform {
                view_proj,
                inv_view_proj,
                camera_pos: frame.camera.position.extend(0.0),
                water_color: glam::Vec4::from_array([
                    cfg.water_color[0],
                    cfg.water_color[1],
                    cfg.water_color[2],
                    1.0,
                ]),
                deep_color: glam::Vec4::from_array([
                    cfg.deep_color[0],
                    cfg.deep_color[1],
                    cfg.deep_color[2],
                    1.0,
                ]),
                wave_direction: glam::Vec2::from_array(cfg.wave_direction),
                wave_amplitude: cfg.wave_amplitude,
                wave_wavelength: cfg.wave_wavelength,
                wave_speed: cfg.wave_speed,
                wave_steepness: cfg.wave_steepness,
                time: self.time,
                level: cfg.level,
                fresnel_power: cfg.fresnel_power,
                refraction_scale: cfg.refraction_scale,
                reflectivity: cfg.reflectivity,
                texture_scale: cfg.texture_scale,
                dudv_strength: cfg.dudv_strength,
                has_dudv: if water.dudv_texture.is_some() { 1 } else { 0 },
                has_normal: if water.normal_texture.is_some() { 1 } else { 0 },
                normal_strength: cfg.normal_strength,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
                _pad3: 0.0,
                sun_direction: {
                    let d = frame.lighting.light.direction;
                    glam::Vec4::new(-d[0], -d[1], -d[2], 0.0)
                },
                sun_color: {
                    let c = frame.lighting.light.color;
                    let i = frame.lighting.light.intensity;
                    glam::Vec4::new(c[0] * i, c[1] * i, c[2] * i, 0.0)
                },
                depth_scale: cfg.depth_scale,
                specular_power: cfg.specular_power,
                secondary_scale: cfg.secondary_scale,
                _pad4: 0.0,
                flow_speed: glam::Vec2::from_array(cfg.flow_speed),
                flow_speed_2: glam::Vec2::from_array(cfg.flow_speed_2),
                reflection_enabled: if cfg.reflection_enabled { 1 } else { 0 },
                reflection_resolution_scale: cfg.reflection_resolution_scale,
                _pad5: 0.0,
                _pad6: 0.0,
            };
            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            // Resolve optional dudv / normal textures. Use the pipeline's neutral
            // fallbacks when no texture is configured so we never sample the cache's
            // fallback-white and accidentally produce full distortion or tilted normals.
            let dudv = match water.dudv_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle, frame.asset_manager),
                None => self.fallback_dudv.clone(),
            };
            let normal = match water.normal_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle, frame.asset_manager),
                None => self.fallback_normal.clone(),
            };

            let needs_rebuild = match (&self.last_dudv, &self.last_normal) {
                (Some(last_d), Some(last_n)) => {
                    !Arc::ptr_eq(last_d, &dudv) || !Arc::ptr_eq(last_n, &normal)
                }
                _ => true,
            };

            if needs_rebuild {
                self.water_texture_bind_group =
                    Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Water Material Bind Group"),
                        layout: &self.water_texture_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&dudv.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&normal.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&self.water_sampler),
                            },
                        ],
                    }));
                self.last_dudv = Some(dudv);
                self.last_normal = Some(normal);
            }
        } else {
            self.has_water = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        execute::execute(self, encoder, resources, _surface_view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::Water;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::FrameConfig;
    use crate::renderer::light::LightingUniforms;
    use crate::renderer::resource::ResourceTag;
    use crate::scene::WaterConfig;

    fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

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

    #[test]
    fn water_pass_signature_reads_lit_scene_depth_and_reflection() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = WaterPass::init(&ctx);
        let sig = pass.signature();
        assert_eq!(sig.name, "Water");
        assert!(sig.reads.iter().any(|s| s.name == SceneColor::NAME));
        assert!(sig.reads.iter().any(|s| s.name == GDepth::NAME));
        assert!(sig.reads.iter().any(|s| s.name == ReflectionTexture::NAME));
        assert!(sig
            .reads
            .iter()
            .any(|s| s.name == WaterReflectionColor::NAME));
        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, WaterColor::NAME);
    }

    #[test]
    fn water_pass_skipped_without_component() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = WaterPass::init(&ctx);
        let world = World::new();
        let optional = extract_optional_pass_data(&world);
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
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
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        assert!(!pass.should_run(&frame));
    }

    #[test]
    fn water_pass_runs_when_component_present() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = WaterPass::init(&ctx);
        let mut world = World::new();
        world.spawn((Water {
            config: WaterConfig::default(),
            dudv_texture: None,
            normal_texture: None,
        },));
        let optional = extract_optional_pass_data(&world);
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
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
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
    }
}
