//! Volumetric Cloud Pass — ray-marched cloud overlay.
//!
//! A full-screen pass that runs after the atmosphere pass and before water/
//! composite. It ray-marches through a horizontal cloud slab, samples a 3D
//! procedural noise texture for density, and writes the result as a separate
//! `CloudColor` overlay. The composite pass blends this overlay over the lit
//! scene.

mod execute;
mod pipeline;
mod types;

/// GPU uniform data for the volumetric cloud shader.
pub use types::CloudUniform;

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{CloudColor, GDepth};
use crate::renderer::resource_table::ResourceTable;
use glam::{Vec3, Vec4};
use types::NOISE_SIZE;

/// Volumetric cloud render pass.
pub struct VolumetricCloudPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    noise_texture: wgpu::Texture,
    noise_view: wgpu::TextureView,
    noise_sampler: wgpu::Sampler,
    noise_data: Vec<u8>,
    noise_uploaded: bool,
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

        // Create the texture bind group: depth + noise + sampler.
        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric Cloud Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.noise_sampler),
                },
            ],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_clouds
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(clouds) = frame.optional.clouds.clone() {
            self.has_clouds = true;
            self.time += frame.delta_time * clouds.config.wind_speed;

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
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            // Upload the noise texture the first time we have a valid queue.
            if !self.noise_uploaded {
                frame.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.noise_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &self.noise_data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(NOISE_SIZE),
                        rows_per_image: Some(NOISE_SIZE),
                    },
                    wgpu::Extent3d {
                        width: NOISE_SIZE,
                        height: NOISE_SIZE,
                        depth_or_array_layers: NOISE_SIZE,
                    },
                );
                self.noise_uploaded = true;
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
    /// Create a new cloud pass, including a CPU-generated 3D noise texture.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut pass = Self::new_without_upload(device);
        // Immediately upload noise so the first frame is ready.
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &pass.noise_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pass.noise_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(NOISE_SIZE),
                rows_per_image: Some(NOISE_SIZE),
            },
            wgpu::Extent3d {
                width: NOISE_SIZE,
                height: NOISE_SIZE,
                depth_or_array_layers: NOISE_SIZE,
            },
        );
        pass.noise_uploaded = true;
        pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let pass = VolumetricCloudPass::new(&device, &queue);
        assert_eq!(pass.noise_texture.width(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.height(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.depth_or_array_layers(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.format(), wgpu::TextureFormat::R8Unorm);
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
}
