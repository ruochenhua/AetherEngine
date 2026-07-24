//! Volumetric Cloud Pass — ray-marched cloud overlay.
//!
//! A full-screen pass that runs after the atmosphere pass and before water/
//! composite. It ray-marches through a horizontal cloud slab, sampling
//! Worley, Perlin-Worley, curl and weather noise textures for density,
//! and writes the result as a separate `CloudColor` overlay. The composite
//! pass blends this overlay over the lit scene.
//!
//! ## Cloud shell placement convention
//!
//! The cloud shell is a spherical shell whose center is placed on the
//! planet's vertical axis **directly below the camera** —
//! `(cam_pos.x, -planet_radius, cam_pos.z)` — not at the camera position
//! itself. If the center followed the camera, the shell would render
//! incorrectly as soon as the camera rises or descends to an arbitrary
//! height. See the inline comment in `apply_frame` (fix commit e7371a0).
//!
//! ## Noise texture lifecycle
//!
//! The Worley / Perlin-Worley / weather noise textures are created lazily:
//! `apply_frame` calls `ensure_noise_textures` the first time a frame carries
//! cloud data, and the textures are regenerated only when the requested
//! `CloudQuality` changes (same quality → no-op). When a frame carries no
//! cloud data (e.g. after switching to a cloudless scene), `apply_frame`
//! calls `clear_noise_textures` so the textures — ~24 MB at High quality —
//! do not stay GPU-resident while unused; they are recreated lazily if
//! clouds reappear.
//!
//! ## `CloudQuality` resolutions
//!
//! `CloudQuality` (Low / Medium / High) scales the procedural noise texture
//! resolutions in `crates/aether-engine/src/clouds/generate.rs`:
//! High = Perlin-Worley 128³ / Worley 32³ / weather 2048², Medium = 64³ /
//! 32³ / 1024², Low = 32³ / 16³ / 512². It does not affect ray-march step
//! counts, which are fixed in the shader.

mod execute;
mod pipeline;
pub(crate) mod shader;
mod textures;
mod types;

/// GPU uniform data for the volumetric cloud shader.
pub use types::CloudUniform;

use crate::renderer::frame::RenderFrame;
use crate::renderer::light::sun_direction_from_lighting;
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

            let cfg = &clouds.config;
            let cam_pos = frame.camera.position;

            // RenderEngine places the sphere center directly below the camera on
            // the planet's vertical axis, so the local cloud shell is always
            // centered above the viewer.
            let planet_radius = cfg.planet_radius;
            let sphere_center = glam::Vec3::new(cam_pos.x, -planet_radius, cam_pos.z);
            let inner_radius = planet_radius + cfg.bottom_altitude;
            let outer_radius = planet_radius + cfg.top_altitude;

            let light_dir = sun_direction_from_lighting(frame.lighting);
            let sun_toward = light_dir;
            let light_factor = sun_toward.dot(Vec3::Y).clamp(0.0, 1.0);

            let raw_light_color = Vec3::from_array(frame.lighting.light.color) * frame.lighting.light.intensity;
            let real_light_color = raw_light_color;

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let inv_view_proj = (proj * view).inverse();

            let uniforms = CloudUniform {
                inv_view_proj,
                camera_pos: Vec4::from((cam_pos, 0.0)),
                sun_direction: Vec4::from((sun_toward, 0.0)),
                sphere_center_inner: Vec4::from((sphere_center, inner_radius)),
                sphere_outer_params: Vec4::new(
                    outer_radius,
                    cfg.max_render_dist,
                    cfg.cloud_top_offset,
                    0.0,
                ),
                wind_time: Vec4::new(
                    cfg.wind_direction[0],
                    0.0,
                    cfg.wind_direction[1],
                    self.time,
                ),
                noise_scales: Vec4::new(
                    cfg.weather_scale,
                    cfg.base_noise_scale,
                    cfg.high_freq_noise_scale,
                    cfg.high_freq_uv_scale,
                ),
                detail_params: Vec4::new(
                    cfg.high_freq_h_scale,
                    cfg.cloud_type,
                    cfg.coverage,
                    0.0,
                ),
                light_color: Vec4::new(
                    real_light_color.x,
                    real_light_color.y,
                    real_light_color.z,
                    light_factor,
                ),
                horizon_color: Vec4::new(0.8, 0.85, 1.0, 0.0),
                zenit_color: Vec4::new(0.0, 0.5, 1.0, 0.0),
                cloud_color: Vec4::new(1.0, 1.0, 1.0, 0.0),
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        } else {
            self.has_clouds = false;
            // The scene has no clouds (e.g. after switching to a cloudless
            // scene): release the noise textures so they do not stay
            // GPU-resident until the pass is dropped. They are recreated
            // lazily by `ensure_noise_textures` if clouds reappear.
            self.clear_noise_textures();
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

    /// Create a new cloud pass with Medium-quality noise textures.
    #[cfg(test)]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::new_with_quality(device, queue, CloudQuality::Medium)
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
        self.weather_texture = Some(resources.weather_texture);
        self.weather_view = Some(resources.weather_view);
        self.multi_noise_sampler = Some(resources.sampler);
        self.noise_bind_group = Some(resources.bind_group);
        self.current_noise_quality = Some(quality);
    }

    /// Release the noise textures, sampler and bind group, freeing their GPU
    /// memory (replace-to-free). A no-op when they are already released.
    /// The textures are recreated lazily by `ensure_noise_textures`.
    fn clear_noise_textures(&mut self) {
        self.worley_texture = None;
        self.worley_view = None;
        self.perlin_worley_texture = None;
        self.perlin_worley_view = None;
        self.weather_texture = None;
        self.weather_view = None;
        self.multi_noise_sampler = None;
        self.noise_bind_group = None;
        self.current_noise_quality = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;
    use crate::ecs::components::Clouds;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::{FrameConfig, RenderFrame};
    use crate::renderer::light::{sun_direction_from_lighting, LightingUniforms};
    use crate::scene::config::CloudConfig;
    use glam::Vec3;
    use std::sync::Arc;

    #[test]
    fn cloud_pass_signature_reads_depth_and_writes_cloud_color() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let sig = VolumetricCloudPass::new(&device, &queue).signature();
        assert_eq!(sig.name, "VolumetricCloud");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn cloud_noise_texture_has_expected_dimensions() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let pass = VolumetricCloudPass::new_with_quality(
            &device,
            &queue,
            CloudQuality::Medium,
        );

        let worley = pass.worley_texture.as_ref().unwrap();
        assert_eq!(worley.width(), 32);
        assert_eq!(worley.height(), 32);
        assert_eq!(worley.depth_or_array_layers(), 32);
        assert_eq!(worley.format(), wgpu::TextureFormat::Rgba8Unorm);

        let perlin_worley = pass.perlin_worley_texture.as_ref().unwrap();
        assert_eq!(perlin_worley.width(), 64);
        assert_eq!(perlin_worley.height(), 64);
        assert_eq!(perlin_worley.depth_or_array_layers(), 64);
        assert_eq!(perlin_worley.format(), wgpu::TextureFormat::Rgba8Unorm);

        let weather = pass.weather_texture.as_ref().unwrap();
        assert_eq!(weather.width(), 1024);
        assert_eq!(weather.height(), 1024);
        assert_eq!(weather.format(), wgpu::TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn cloud_noise_texture_dimensions_scale_with_quality() {
        fn sizes(
            pass: &VolumetricCloudPass,
        ) -> ((u32, u32, u32), (u32, u32, u32), (u32, u32)) {
            let w = pass.worley_texture.as_ref().unwrap();
            assert_eq!(w.format(), wgpu::TextureFormat::Rgba8Unorm);
            let pw = pass.perlin_worley_texture.as_ref().unwrap();
            assert_eq!(pw.format(), wgpu::TextureFormat::Rgba8Unorm);
            let wt = pass.weather_texture.as_ref().unwrap();
            assert_eq!(wt.format(), wgpu::TextureFormat::Rgba8Unorm);
            (
                (pw.width(), pw.height(), pw.depth_or_array_layers()),
                (w.width(), w.height(), w.depth_or_array_layers()),
                (wt.width(), wt.height()),
            )
        }

        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };

        let low = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::Low,
        );
        assert_eq!(sizes(&low), ((32, 32, 32), (16, 16, 16), (512, 512)));

        let medium = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::Medium,
        );
        assert_eq!(
            sizes(&medium),
            ((64, 64, 64), (32, 32, 32), (1024, 1024))
        );

        let high = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::High,
        );
        assert_eq!(sizes(&high), ((128, 128, 128), (32, 32, 32), (2048, 2048)));
    }

    #[test]
    fn cloud_noise_textures_regenerated_when_quality_changes() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut pass = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::Low,
        );

        // Same quality: no-op, dimensions unchanged.
        pass.ensure_noise_textures(&queue, CloudQuality::Low);
        assert_eq!(pass.perlin_worley_texture.as_ref().unwrap().width(), 32);

        // Different quality: textures recreated with new dimensions.
        pass.ensure_noise_textures(&queue, CloudQuality::High);
        assert_eq!(pass.perlin_worley_texture.as_ref().unwrap().width(), 128);
        assert_eq!(pass.worley_texture.as_ref().unwrap().width(), 32);
        assert_eq!(pass.weather_texture.as_ref().unwrap().width(), 2048);
    }

    #[test]
    fn cloud_noise_textures_released_and_lazily_recreated() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut pass = VolumetricCloudPass::new_with_quality(
            &device, &queue, CloudQuality::High,
        );
        assert!(pass.perlin_worley_texture.is_some());

        // Releasing clears every noise-texture field so the GPU memory is freed.
        pass.clear_noise_textures();
        assert!(pass.worley_texture.is_none());
        assert!(pass.worley_view.is_none());
        assert!(pass.perlin_worley_texture.is_none());
        assert!(pass.perlin_worley_view.is_none());
        assert!(pass.weather_texture.is_none());
        assert!(pass.weather_view.is_none());
        assert!(pass.multi_noise_sampler.is_none());
        assert!(pass.noise_bind_group.is_none());
        assert_eq!(pass.current_noise_quality, None);

        // Releasing twice is a no-op; a later ensure recreates the textures.
        pass.clear_noise_textures();
        pass.ensure_noise_textures(&queue, CloudQuality::Low);
        assert_eq!(pass.perlin_worley_texture.as_ref().unwrap().width(), 32);
        assert!(pass.noise_bind_group.is_some());
        assert_eq!(pass.current_noise_quality, Some(CloudQuality::Low));
    }

    #[test]
    fn cloud_pass_releases_noise_textures_on_cloudless_frame() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut pass = VolumetricCloudPass::new(&device, &queue);
        assert!(pass.perlin_worley_texture.is_some());

        // A frame without a Clouds component (e.g. after switching to a
        // cloudless scene) must release the noise textures.
        let world = World::new();
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
            terrain_geometry: None,
            texture_cache: &texture_cache,
            asset_manager: &assets,
        };

        pass.apply_frame(&frame);
        assert!(!pass.has_clouds);
        assert!(pass.perlin_worley_texture.is_none());
        assert!(pass.noise_bind_group.is_none());
        assert_eq!(pass.current_noise_quality, None);
    }

    #[test]
    fn cloud_pass_runs_when_cloud_component_present() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
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
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let pass = VolumetricCloudPass::new(&device, &queue);
        // Without apply_frame being called, has_clouds remains false.
        assert!(!pass.has_clouds);
    }

    #[test]
    fn cloud_pass_applies_frame_with_high_quality() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
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
            terrain_geometry: None,
            texture_cache: &texture_cache,
            asset_manager: &assets,
        };

        pass.apply_frame(&frame);
        assert!(pass.has_clouds);

        // Read back the uniform buffer and verify the spherical-shell parameters.
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
        assert_eq!(uniforms[0].sphere_outer_params.x, 6480.0);
        assert_eq!(uniforms[0].noise_scales.w, 150.0);
        assert!((uniforms[0].light_color.w - 0.57735).abs() < 0.001);

        // The sun direction written to the shader must match the shared helper.
        let expected_sun = sun_direction_from_lighting(&lighting);
        let actual_sun = Vec3::new(
            uniforms[0].sun_direction.x,
            uniforms[0].sun_direction.y,
            uniforms[0].sun_direction.z,
        );
        assert!(
            actual_sun.abs_diff_eq(expected_sun, 1e-4),
            "expected sun_direction {expected_sun:?}, got {actual_sun:?}"
        );
    }
}
