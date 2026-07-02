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
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    noise_bind_group_layout: wgpu::BindGroupLayout,
    noise_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    // Multi-noise textures (bind group 2), created lazily from scene quality.
    worley_texture: Option<wgpu::Texture>,
    worley_view: Option<wgpu::TextureView>,
    perlin_worley_texture: Option<wgpu::Texture>,
    perlin_worley_view: Option<wgpu::TextureView>,
    curl_texture: Option<wgpu::Texture>,
    curl_view: Option<wgpu::TextureView>,
    weather_texture: Option<wgpu::Texture>,
    weather_view: Option<wgpu::TextureView>,
    multi_noise_sampler: Option<wgpu::Sampler>,
    current_noise_quality: Option<CloudQuality>,
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
        Self::new_without_upload(ctx.device)
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
            self.ensure_noise_textures(frame.queue, quality);

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
                // Fixed defaults until scene config exposes cloud color fields.
                cloud_color_low: CloudUniform::default().cloud_color_low,
                cloud_color_high: CloudUniform::default().cloud_color_high,
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
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

    /// Create a new cloud pass with Medium-quality noise textures.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut pass = Self::new_without_upload(device);
        pass.ensure_noise_textures(queue, CloudQuality::Medium);
        pass
    }

    /// Create a new cloud pass with the requested quality noise textures.
    #[cfg(test)]
    pub fn new_with_quality(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quality: CloudQuality,
    ) -> Self {
        let mut pass = Self::new_without_upload(device);
        pass.ensure_noise_textures(queue, quality);
        pass
    }

    /// Lazily create (or recreate) the noise textures and bind group for the
    /// requested quality. If the textures already exist with the same quality,
    /// this is a no-op.
    fn ensure_noise_textures(
        &mut self,
        queue: &wgpu::Queue,
        quality: CloudQuality,
    ) {
        if self.current_noise_quality == Some(quality) {
            return;
        }

        let resources = textures::create_noise_resources(
            &self.device,
            queue,
            quality,
            &self.noise_bind_group_layout,
        );

        self.worley_texture = Some(resources.worley_texture);
        self.worley_view = Some(resources.worley_view);
        self.perlin_worley_texture = Some(resources.perlin_worley_texture);
        self.perlin_worley_view = Some(resources.perlin_worley_view);
        self.curl_texture = Some(resources.curl_texture);
        self.curl_view = Some(resources.curl_view);
        self.weather_texture = Some(resources.weather_texture);
        self.weather_view = Some(resources.weather_view);
        self.multi_noise_sampler = Some(resources.sampler);
        self.noise_bind_group = Some(resources.bind_group);
        self.current_noise_quality = Some(quality);
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
        let sig = VolumetricCloudPass::new(&device, &queue).signature();
        assert_eq!(sig.name, "VolumetricCloud");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn cloud_noise_texture_has_expected_dimensions() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new_with_quality(
            &device,
            &queue,
            CloudQuality::Medium,
        );

        let worley = pass.worley_texture.as_ref().unwrap();
        assert_eq!(worley.width(), 128);
        assert_eq!(worley.depth_or_array_layers(), 128);
        assert_eq!(worley.format(), wgpu::TextureFormat::R8Unorm);

        let perlin_worley = pass.perlin_worley_texture.as_ref().unwrap();
        assert_eq!(perlin_worley.width(), 128);
        assert_eq!(perlin_worley.depth_or_array_layers(), 128);
        assert_eq!(perlin_worley.format(), wgpu::TextureFormat::R8Unorm);

        let curl = pass.curl_texture.as_ref().unwrap();
        assert_eq!(curl.width(), 16);
        assert_eq!(curl.depth_or_array_layers(), 16);
        assert_eq!(curl.format(), wgpu::TextureFormat::Rg8Snorm);

        let weather = pass.weather_texture.as_ref().unwrap();
        assert_eq!(weather.width(), 64);
        assert_eq!(weather.height(), 64);
        assert_eq!(weather.format(), wgpu::TextureFormat::R8Unorm);
    }

    #[test]
    fn cloud_noise_texture_dimensions_vary_by_quality() {
        fn assert_sizes(pass: &VolumetricCloudPass, worley: u32, curl: u32, weather: u32) {
            let w = pass.worley_texture.as_ref().unwrap();
            assert_eq!(w.width(), worley);
            assert_eq!(w.height(), worley);
            assert_eq!(w.depth_or_array_layers(), worley);

            let pw = pass.perlin_worley_texture.as_ref().unwrap();
            assert_eq!(pw.width(), worley);
            assert_eq!(pw.height(), worley);
            assert_eq!(pw.depth_or_array_layers(), worley);

            let c = pass.curl_texture.as_ref().unwrap();
            assert_eq!(c.width(), curl);
            assert_eq!(c.height(), curl);
            assert_eq!(c.depth_or_array_layers(), curl);

            let wt = pass.weather_texture.as_ref().unwrap();
            assert_eq!(wt.width(), weather);
            assert_eq!(wt.height(), weather);
        }

        let (device, queue) = headless_device_queue();
        let low = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::Low,
        );
        assert_sizes(&low, 64, 16, 32);

        let medium = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::Medium,
        );
        assert_sizes(&medium, 128, 16, 64);

        let high = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::High,
        );
        assert_sizes(&high, 128, 32, 64);
    }

    #[test]
    fn cloud_pass_runs_when_cloud_component_present() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue);
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
        let pass = VolumetricCloudPass::new(&device, &queue);
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
        let mut pass = VolumetricCloudPass::new(&device, &queue);
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
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue,
        );
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

        // Read back the uniform buffer and verify the high-quality step count
        // was written.
        let uniform_size = std::mem::size_of::<CloudUniform>() as wgpu::BufferAddress;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cloud uniform readback"),
                size: uniform_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            },
        );
        let mut encoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("cloud uniform readback"),
            },
        );
        encoder.copy_buffer_to_buffer(
            &pass.uniform_buffer,
            0,
            &staging,
            0,
            uniform_size,
        );
        queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("failed to map uniform readback buffer");
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let data = slice.get_mapped_range();
        let uniforms: &[CloudUniform] = bytemuck::cast_slice(&data);
        assert_eq!(uniforms[0].quality_params.x, 128.0);
    }
}
