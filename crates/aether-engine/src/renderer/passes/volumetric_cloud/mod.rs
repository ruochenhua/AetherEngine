//! Volumetric Cloud Pass — ray-marched cloud overlay.
//!
//! A full-screen pass that runs after the atmosphere pass and before water/
//! composite. It ray-marches through a horizontal cloud slab, sampling
//! Worley, Perlin-Worley, curl and weather noise textures for density,
//! and writes the result as a separate `CloudColor` overlay. The composite
//! pass blends this overlay over the lit scene.

mod execute;
mod pipeline;
mod shader;
mod textures;
mod types;

/// GPU uniform data for the volumetric cloud shader.
pub use types::CloudUniform;

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{CloudColor, GDepth};
use crate::renderer::resource_table::ResourceTable;
use crate::scene::config::CloudQuality;
use glam::{Vec3, Vec4};

/// Volumetric cloud render pass.
pub struct VolumetricCloudPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    noise_bind_group_layout: wgpu::BindGroupLayout,
    noise_bind_group: wgpu::BindGroup,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    // Multi-noise textures (bind group 2).
    worley_texture: wgpu::Texture,
    #[allow(dead_code)]
    worley_view: wgpu::TextureView,
    perlin_worley_texture: wgpu::Texture,
    #[allow(dead_code)]
    perlin_worley_view: wgpu::TextureView,
    curl_texture: wgpu::Texture,
    #[allow(dead_code)]
    curl_view: wgpu::TextureView,
    weather_texture: wgpu::Texture,
    #[allow(dead_code)]
    weather_view: wgpu::TextureView,
    #[allow(dead_code)]
    multi_noise_sampler: wgpu::Sampler,
    worley_data: Vec<u8>,
    perlin_worley_data: Vec<u8>,
    curl_data: Vec<[i8; 2]>,
    weather_data: Vec<u8>,
    multi_noise_uploaded: bool,
    depth_handle: Option<ResHandle<GDepth>>,
    cloud_color_handle: Option<ResHandle<CloudColor>>,
    has_clouds: bool,
    time: f32,
}

impl Pass for VolumetricCloudPass {
    fn name(&self) -> &str {
        "VolumetricCloud"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("VolumetricCloud")
            .read::<GDepth>()
            .write::<CloudColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new_without_upload(ctx.device, &CloudQuality::Medium)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.depth_handle = Some(resources.handle::<GDepth>());
        self.cloud_color_handle = Some(resources.handle::<CloudColor>());

        let depth_view = resources.get(self.depth_handle.unwrap());
        let _cloud_color_view = resources.get(self.cloud_color_handle.unwrap());

        // Create the texture bind group: depth only.
        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric Cloud Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth_view),
            }],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_clouds
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(clouds) = frame.optional.clouds.clone() {
            self.has_clouds = true;
            self.time += frame.delta_time * clouds.config.wind_speed;

            let quality = clouds.config.quality;
            let quality_params = Self::quality_params(&quality);

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let inv_view_proj = (proj * view).inverse();

            let light_dir = Vec3::from_array(frame.lighting.light.direction).normalize();
            let sun_toward = -light_dir;

            let cfg = &clouds.config;
            let uniforms = CloudUniform {
                inv_view_proj,
                camera_pos: Vec4::from((frame.camera.position, 0.0)),
                sun_direction: Vec4::from((sun_toward, 0.0)),
                cloud_bounds: Vec4::new(
                    cfg.bottom_altitude,
                    cfg.top_altitude,
                    cfg.coverage,
                    cfg.density,
                ),
                wind_time: Vec4::new(cfg.wind_direction[0], cfg.wind_direction[1], 0.0, self.time),
                quality_params,
                cloud_color_low: CloudUniform::default().cloud_color_low,
                cloud_color_high: CloudUniform::default().cloud_color_high,
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            // Upload the multi-noise textures the first time we have a valid queue.
            if !self.multi_noise_uploaded {
                self.upload_multi_noise(frame.queue);
            }
        } else {
            self.has_clouds = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        self.record_render_pass(encoder, resources);
    }
}

impl VolumetricCloudPass {
    /// Map a `CloudQuality` preset to the shader `quality_params` uniform.
    ///
    /// X = primary ray-march steps, Y = shadow steps, Z = forward Henyey-Greenstein
    /// g, W = back-scattering g.
    fn quality_params(quality: &CloudQuality) -> glam::Vec4 {
        match quality {
            CloudQuality::Low => glam::Vec4::new(32.0, 4.0, 0.85, 0.3),
            CloudQuality::Medium => glam::Vec4::new(64.0, 6.0, 0.85, 0.3),
            CloudQuality::High => glam::Vec4::new(128.0, 8.0, 0.85, 0.3),
        }
    }

    /// Create a new cloud pass, including CPU-generated multi-noise textures.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, quality: CloudQuality) -> Self {
        let mut pass = Self::new_without_upload(device, &quality);
        // Immediately upload noise so the first frame is ready.
        pass.upload_multi_noise(queue);
        pass
    }

    fn upload_multi_noise(&mut self, queue: &wgpu::Queue) {
        let worley_size = self.worley_texture.width();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.worley_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.worley_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(worley_size),
                rows_per_image: Some(worley_size),
            },
            wgpu::Extent3d {
                width: worley_size,
                height: worley_size,
                depth_or_array_layers: worley_size,
            },
        );

        let perlin_worley_size = self.perlin_worley_texture.width();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.perlin_worley_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.perlin_worley_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(perlin_worley_size),
                rows_per_image: Some(perlin_worley_size),
            },
            wgpu::Extent3d {
                width: perlin_worley_size,
                height: perlin_worley_size,
                depth_or_array_layers: perlin_worley_size,
            },
        );

        let curl_size = self.curl_texture.width();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.curl_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.curl_data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(curl_size * 2),
                rows_per_image: Some(curl_size),
            },
            wgpu::Extent3d {
                width: curl_size,
                height: curl_size,
                depth_or_array_layers: curl_size,
            },
        );

        let weather_size = self.weather_texture.width();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.weather_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.weather_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(weather_size),
                rows_per_image: Some(weather_size),
            },
            wgpu::Extent3d {
                width: weather_size,
                height: weather_size,
                depth_or_array_layers: 1,
            },
        );

        self.multi_noise_uploaded = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::Clouds;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::{FrameConfig, RenderFrame};
    use crate::renderer::light::LightingUniforms;
    use crate::scene::config::CloudConfig;
    use std::sync::Arc;

    fn headless_device_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn cloud_pass_signature_reads_depth_and_writes_cloud_color() {
        let (device, queue) = headless_device_queue();
        let sig = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium).signature();
        assert_eq!(sig.name, "VolumetricCloud");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn cloud_noise_texture_has_expected_dimensions() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium);

        assert_eq!(pass.worley_texture.width(), 128);
        assert_eq!(pass.worley_texture.depth_or_array_layers(), 128);
        assert_eq!(pass.worley_texture.format(), wgpu::TextureFormat::R8Unorm);

        assert_eq!(pass.perlin_worley_texture.width(), 128);
        assert_eq!(pass.perlin_worley_texture.depth_or_array_layers(), 128);
        assert_eq!(
            pass.perlin_worley_texture.format(),
            wgpu::TextureFormat::R8Unorm
        );

        assert_eq!(pass.curl_texture.width(), 16);
        assert_eq!(pass.curl_texture.depth_or_array_layers(), 16);
        assert_eq!(pass.curl_texture.format(), wgpu::TextureFormat::Rg8Snorm);

        assert_eq!(pass.weather_texture.width(), 64);
        assert_eq!(pass.weather_texture.height(), 64);
        assert_eq!(pass.weather_texture.format(), wgpu::TextureFormat::R8Unorm);
    }

    #[test]
    fn cloud_noise_texture_dimensions_vary_by_quality() {
        fn assert_sizes(pass: &VolumetricCloudPass, worley: u32, curl: u32, weather: u32) {
            assert_eq!(pass.worley_texture.width(), worley);
            assert_eq!(pass.worley_texture.height(), worley);
            assert_eq!(pass.worley_texture.depth_or_array_layers(), worley);

            assert_eq!(pass.perlin_worley_texture.width(), worley);
            assert_eq!(pass.perlin_worley_texture.height(), worley);
            assert_eq!(pass.perlin_worley_texture.depth_or_array_layers(), worley);

            assert_eq!(pass.curl_texture.width(), curl);
            assert_eq!(pass.curl_texture.height(), curl);
            assert_eq!(pass.curl_texture.depth_or_array_layers(), curl);

            assert_eq!(pass.weather_texture.width(), weather);
            assert_eq!(pass.weather_texture.height(), weather);
        }

        let (device, queue) = headless_device_queue();
        let low = VolumetricCloudPass::new(&device, &queue, CloudQuality::Low);
        assert_sizes(&low, 64, 16, 32);

        let medium = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium);
        assert_sizes(&medium, 128, 16, 64);

        let high = VolumetricCloudPass::new(&device, &queue, CloudQuality::High);
        assert_sizes(&high, 128, 32, 64);
    }

    #[test]
    fn cloud_pass_runs_when_cloud_component_present() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium);
        assert!(pass
            .signature()
            .writes
            .iter()
            .any(|s| s.name == "cloud_color"));
        assert!(pass
            .signature()
            .reads
            .iter()
            .any(|s| s.name == "gbuffer_depth"));
    }

    #[test]
    fn cloud_pass_skipped_without_component() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium);
        // Without apply_frame being called, has_clouds remains false.
        assert!(!pass.has_clouds);
    }

    #[test]
    fn quality_preset_maps_to_step_counts() {
        let low = VolumetricCloudPass::quality_params(&CloudQuality::Low);
        let med = VolumetricCloudPass::quality_params(&CloudQuality::Medium);
        let high = VolumetricCloudPass::quality_params(&CloudQuality::High);
        assert!(low.x < med.x);
        assert!(med.x < high.x);
    }

    #[test]
    fn cloud_pass_applies_frame_with_high_quality() {
        let (device, queue) = headless_device_queue();
        let mut pass = VolumetricCloudPass::new(&device, &queue, CloudQuality::Medium);
        let mut world = World::new();
        world.spawn((Clouds {
            config: CloudConfig {
                quality: CloudQuality::High,
                ..CloudConfig::default()
            },
        },));
        let optional = extract_optional_pass_data(&world);

        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let assets = crate::asset::AssetManager::new();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let frame = RenderFrame {
            batches: Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
            texture_cache: &texture_cache,
            asset_manager: &assets,
        };

        pass.apply_frame(&frame);
        assert!(pass.has_clouds);
    }
}
