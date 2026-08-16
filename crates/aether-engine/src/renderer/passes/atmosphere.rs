//! Atmosphere Pass — physically based sky scattering.
//!
//! A full-screen pass that runs after deferred lighting and replaces the
//! environment-map sky background with Rayleigh + Mie scattering. Geometry
//! pixels (identified by G-Buffer position alpha > 0) are preserved from the
//! input `SceneColor`.

use crate::renderer::pass::ResHandle;
use crate::renderer::resource::{GDepth, SceneColor};

mod pass;
mod pipeline;
mod shaders;

/// GPU uniform data for the atmosphere shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtmosphereUniform {
    /// Direction toward the sun (world space).
    pub sun_direction: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad0: f32,
    /// Camera world-space position.
    pub camera_pos: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad1: f32,
    /// Planet radius in world units.
    pub planet_radius: f32,
    /// Atmosphere shell thickness.
    pub atmosphere_height: f32,
    /// Rayleigh density scale height.
    pub rayleigh_scale_height: f32,
    /// Mie density scale height.
    pub mie_scale_height: f32,
    /// Rayleigh scattering coefficients (RGB).
    pub rayleigh_scattering: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad2: f32,
    /// Mie scattering coefficients (RGB).
    pub mie_scattering: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad3: f32,
    /// Sun intensity multiplier.
    pub sun_intensity: f32,
    /// Mie asymmetry parameter (g).
    pub mie_asymmetry: f32,
    /// Padding to 16-byte alignment.
    pub _pad4: f32,
    /// Padding to 16-byte alignment.
    pub _pad5: f32,
    /// Ozone absorption coefficients (RGB).
    pub ozone_absorption: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad6: f32,
    /// Ozone layer scale height (tent half-width).
    pub ozone_scale_height: f32,
    /// Padding to 16-byte alignment.
    pub _pad7: f32,
    /// Padding to 16-byte alignment.
    pub _pad8: f32,
    /// Padding to 16-byte alignment.
    pub _pad9: f32,
    /// Approximate multiple-scattering strength.
    pub multi_scattering_factor: f32,
    /// Padding to 16-byte alignment.
    pub _pad10: f32,
    /// Padding to 16-byte alignment.
    pub _pad11: f32,
    /// Padding to 16-byte alignment.
    pub _pad12: f32,
    /// Inverse view-projection matrix for view-ray reconstruction.
    pub inv_view_proj: [[f32; 4]; 4],
}

impl Default for AtmosphereUniform {
    fn default() -> Self {
        Self {
            sun_direction: [0.0, 0.2, -1.0],
            _pad0: 0.0,
            camera_pos: [0.0; 3],
            _pad1: 0.0,
            planet_radius: 6360.0,
            atmosphere_height: 100.0,
            rayleigh_scale_height: 8.0,
            mie_scale_height: 1.2,
            rayleigh_scattering: [0.005802, 0.013558, 0.033100],
            _pad2: 0.0,
            mie_scattering: [0.001, 0.001, 0.001],
            _pad3: 0.0,
            sun_intensity: 10.0,
            mie_asymmetry: 0.82,
            _pad4: 0.0,
            _pad5: 0.0,
            ozone_absorption: [0.0005, 0.001, 0.0001],
            _pad6: 0.0,
            ozone_scale_height: 15.0,
            _pad7: 0.0,
            _pad8: 0.0,
            _pad9: 0.0,
            multi_scattering_factor: 0.1,
            _pad10: 0.0,
            _pad11: 0.0,
            _pad12: 0.0,
            inv_view_proj: [[0.0; 4]; 4],
        }
    }
}

/// Atmosphere render pass.
pub struct AtmospherePass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    depth_handle: Option<ResHandle<GDepth>>,
    has_atmosphere: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::Atmosphere;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::FrameConfig;
    use crate::renderer::frame::RenderFrame;
    use crate::renderer::light::LightingUniforms;
    use crate::renderer::pass::{InitContext, Pass};
    use crate::renderer::resource::ResourceTag;
    use crate::scene::AtmosphereConfig;
    use glam::Vec3;

    async fn read_uniform_buffer(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: &wgpu::Buffer,
    ) -> Vec<u8> {
        let size = std::mem::size_of::<AtmosphereUniform>() as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Atmosphere Uniform Readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Atmosphere Uniform Copy"),
        });
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        slice.get_mapped_range().to_vec()
    }

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
    fn atmosphere_pass_signature_reads_depth_and_writes_scene_color() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = AtmospherePass::init(&ctx);
        let sig = pass.signature();
        assert_eq!(sig.name, "Atmosphere");
        assert!(sig.reads.iter().any(|s| s.name == GDepth::NAME));
        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, SceneColor::NAME);
    }

    #[test]
    fn atmosphere_pass_skipped_without_component() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = AtmospherePass::init(&ctx);
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
            terrain_geometry: None,
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        assert!(!pass.should_run(&frame));
    }

    #[test]
    fn atmosphere_pass_runs_when_component_present() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = AtmospherePass::init(&ctx);
        let mut world = World::new();
        world.spawn((Atmosphere {
            config: AtmosphereConfig::default(),
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
            terrain_geometry: None,
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
    }

    #[test]
    fn atmosphere_pass_uses_light_direction_for_sun() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = AtmospherePass::init(&ctx);
        let mut world = World::new();

        // Any configured sun_direction in AtmosphereConfig is ignored; the pass
        // must derive the sun direction from the scene's directional light.
        world.spawn((Atmosphere {
            config: AtmosphereConfig {
                sun_direction: [0.5, 0.3, -0.8],
                ..Default::default()
            },
        },));

        let optional = extract_optional_pass_data(&world);
        let camera = FlyCamera::default();
        let mut lighting = LightingUniforms::default();
        lighting.light.direction = [-0.2, -0.9, -0.3];
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
        pass.apply_frame(&frame);

        let bytes = pollster::block_on(read_uniform_buffer(&device, &queue, &pass.uniform_buffer));
        let uniform: AtmosphereUniform = *bytemuck::from_bytes(&bytes);
        let expected = -Vec3::from_array(lighting.light.direction).normalize();
        let actual = Vec3::from_array(uniform.sun_direction);
        assert!(
            actual.abs_diff_eq(expected, 1e-4),
            "expected sun_direction {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn atmosphere_pass_uses_configured_ozone_parameters() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = AtmospherePass::init(&ctx);
        let mut world = World::new();

        let absorption = [0.0006f32, 0.0012, 0.00015];
        let scale_height = 18.0;
        world.spawn((Atmosphere {
            config: AtmosphereConfig {
                ozone_absorption: absorption,
                ozone_scale_height: scale_height,
                ..Default::default()
            },
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
            terrain_geometry: None,
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        pass.apply_frame(&frame);

        let bytes = pollster::block_on(read_uniform_buffer(&device, &queue, &pass.uniform_buffer));
        let uniform: AtmosphereUniform = *bytemuck::from_bytes(&bytes);
        assert_eq!(
            uniform.ozone_absorption, absorption,
            "expected ozone_absorption {absorption:?}, got {:?}",
            uniform.ozone_absorption
        );
        assert!(
            (uniform.ozone_scale_height - scale_height).abs() < 1e-4,
            "expected ozone_scale_height {scale_height}, got {}",
            uniform.ozone_scale_height
        );
    }

    #[test]
    fn atmosphere_pass_uses_configured_multi_scattering_factor() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = AtmospherePass::init(&ctx);
        let mut world = World::new();

        let factor = 0.35;
        world.spawn((Atmosphere {
            config: AtmosphereConfig {
                multi_scattering_factor: factor,
                ..Default::default()
            },
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
            terrain_geometry: None,
            texture_cache: ctx.texture_cache,
            asset_manager: &assets,
        };
        pass.apply_frame(&frame);

        let bytes = pollster::block_on(read_uniform_buffer(&device, &queue, &pass.uniform_buffer));
        let uniform: AtmosphereUniform = *bytemuck::from_bytes(&bytes);
        assert!(
            (uniform.multi_scattering_factor - factor).abs() < 1e-4,
            "expected multi_scattering_factor {factor}, got {}",
            uniform.multi_scattering_factor
        );
    }
}
