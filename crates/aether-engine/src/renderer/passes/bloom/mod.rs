//! Bloom Pass
//!
//! Multi-pass screen-space bloom effect. Extracts bright regions,
//! downsample-blurs through 3 mip levels, upsample-additively blends back,
//! and composites with the original HDR image.
//!
//! Pipeline position: CompositePass → BloomPass → ToneMappingPass
//!
//! Internally executes 7 sub-passes in sequence:
//!   1. Extract     PostProcessInput → BrightTexture
//!   2. Downsample  BrightTexture    → BloomMip0 (1/2)
//!   3. Downsample  BloomMip0        → BloomMip1 (1/4)
//!   4. Downsample  BloomMip1        → BloomMip2 (1/8)
//!   5. Upsample    BloomMip2        → BloomMip1 (add)
//!   6. Upsample    BloomMip1        → BloomMip0 (add)
//!   7. Upsample    BloomMip0        → BloomTexture (add)
//!   8. Composite   PostProcessInput + BloomTexture → BloomResult

use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;

mod execute;
mod pipelines;
mod shaders;
mod textures;

/// Bloom parameters (matches WGSL std140 layout).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
}

/// Bloom Pass implementation.
pub struct BloomPass {
    // Sub-pass pipelines
    extract_pipeline: wgpu::RenderPipeline,
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,

    // Shared geometry
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,

    // Uniforms
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,

    // Bind group layouts
    extract_bgl: wgpu::BindGroupLayout,
    blur_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,

    // Texture bind groups (recreated in resolve)
    extract_bg: Option<wgpu::BindGroup>,
    downsample0_bg: Option<wgpu::BindGroup>,
    downsample1_bg: Option<wgpu::BindGroup>,
    downsample2_bg: Option<wgpu::BindGroup>,
    upsample0_bg: Option<wgpu::BindGroup>,
    upsample1_bg: Option<wgpu::BindGroup>,
    upsample2_bg: Option<wgpu::BindGroup>,
    composite_bg: Option<wgpu::BindGroup>,

    // Samplers
    sampler_linear_clamp: wgpu::Sampler,
    sampler_linear_wrap: wgpu::Sampler,

    // Resource handles (from ResourceTable)
    input_handle: Option<ResHandle<PostProcessInput>>,
    result_handle: Option<ResHandle<BloomResult>>,

    // Screen dimensions (updated via set_screen_size)
    screen_width: u32,
    screen_height: u32,

    // Intermediate textures and views (created in resolve)
    bright_texture: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip0: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip1: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip2: Option<(wgpu::Texture, wgpu::TextureView)>,
    bloom_texture: Option<(wgpu::Texture, wgpu::TextureView)>,

    // Parameters
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: bool,
}

impl Pass for BloomPass {
    fn name(&self) -> &str {
        "Bloom"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Bloom")
            .read::<PostProcessInput>("post_process_input")
            .write::<BloomResult>("bloom_result", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device, 1280, 720)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.input_handle = Some(resources.handle::<PostProcessInput>("post_process_input"));
        self.result_handle = Some(resources.handle::<BloomResult>("bloom_result"));

        // Recreate intermediate textures at current screen size
        textures::create_intermediate_textures(self, device);

        let input_view = resources.get(self.input_handle.unwrap());
        let bright_view = &self.bright_texture.as_ref().unwrap().1;
        let mip0_view = &self.mip0.as_ref().unwrap().1;
        let mip1_view = &self.mip1.as_ref().unwrap().1;
        let mip2_view = &self.mip2.as_ref().unwrap().1;
        let bloom_view = &self.bloom_texture.as_ref().unwrap().1;

        // Extract bind group
        self.extract_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Extract BG"),
            layout: &self.extract_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        // Downsample bind groups
        self.downsample0_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 0 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(bright_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        self.downsample1_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 1 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        self.downsample2_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 2 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        // Upsample bind groups (read from lower res, write to higher res with additive blend)
        self.upsample0_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 0 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        self.upsample1_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 1 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        self.upsample2_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 2 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        // Composite bind group
        self.composite_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Composite BG"),
            layout: &self.composite_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        surface_view: &wgpu::TextureView,
    ) {
        execute::execute(self, encoder, resources, surface_view);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl BloomPass {
    /// Create a new bloom pass.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let objects = pipelines::create_device_objects(device);

        let mut pass = Self {
            extract_pipeline: objects.extract_pipeline,
            downsample_pipeline: objects.downsample_pipeline,
            upsample_pipeline: objects.upsample_pipeline,
            composite_pipeline: objects.composite_pipeline,
            quad_vertex_buffer: objects.quad_vertex_buffer,
            quad_vertex_count: objects.quad_vertex_count,
            uniform_buffer: objects.uniform_buffer,
            uniform_bind_group: objects.uniform_bind_group,
            uniform_bind_group_layout: objects.uniform_bind_group_layout,
            extract_bgl: objects.extract_bgl,
            blur_bgl: objects.blur_bgl,
            composite_bgl: objects.composite_bgl,
            extract_bg: None,
            downsample0_bg: None,
            downsample1_bg: None,
            downsample2_bg: None,
            upsample0_bg: None,
            upsample1_bg: None,
            upsample2_bg: None,
            composite_bg: None,
            sampler_linear_clamp: objects.sampler_linear_clamp,
            sampler_linear_wrap: objects.sampler_linear_wrap,
            input_handle: None,
            result_handle: None,
            screen_width: width,
            screen_height: height,
            bright_texture: None,
            mip0: None,
            mip1: None,
            mip2: None,
            bloom_texture: None,
            threshold: 1.0,
            intensity: 1.0,
            bloom_intensity: 0.5,
            enabled: true,
        };

        textures::create_intermediate_textures(&mut pass, device);
        pass
    }

    /// Set screen dimensions (call before rebuild on resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Enable/disable bloom.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set extraction threshold.
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    /// Set extraction intensity.
    pub fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity;
    }

    /// Set final bloom intensity.
    pub fn set_bloom_intensity(&mut self, intensity: f32) {
        self.bloom_intensity = intensity;
    }

    /// Update uniform buffer with current parameters.
    pub fn update_uniforms(&self, queue: &wgpu::Queue) {
        let uniforms = BloomUniforms {
            threshold: self.threshold,
            intensity: self.intensity,
            bloom_intensity: self.bloom_intensity,
            enabled: if self.enabled { 1 } else { 0 },
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    #[test]
    fn signature_declares_resources() {
        let device = headless_device();
        let pass = BloomPass::new(&device, 64, 64);
        let sig = pass.signature();
        assert_eq!(sig.name, "Bloom");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn init_creates_resources() {
        let _pass = BloomPass::new(&headless_device(), 64, 64);
    }

    #[test]
    fn default_params() {
        let device = headless_device();
        let pass = BloomPass::new(&device, 64, 64);
        assert!(pass.enabled);
        assert_eq!(pass.threshold, 1.0);
        assert_eq!(pass.intensity, 1.0);
        assert_eq!(pass.bloom_intensity, 0.5);
    }

    #[test]
    fn params_can_be_changed() {
        let device = headless_device();
        let mut pass = BloomPass::new(&device, 64, 64);
        pass.set_enabled(false);
        pass.set_threshold(2.0);
        pass.set_intensity(3.0);
        pass.set_bloom_intensity(0.8);
        assert!(!pass.enabled);
        assert_eq!(pass.threshold, 2.0);
        assert_eq!(pass.intensity, 3.0);
        assert_eq!(pass.bloom_intensity, 0.8);
    }
}
