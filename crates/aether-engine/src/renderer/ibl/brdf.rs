// BRDF integration LUT compute pass.

use super::cubemap::CpuCubemap;
use super::shaders::BRDF_LUT_SHADER;

impl CpuCubemap {
    /// BRDF integration LUT via compute shader.
    pub(super) fn brdf_integration(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lut_tex: &wgpu::Texture,
        size: u32,
    ) {
        Self::brdf_lut_debug(device, queue, lut_tex, size);
    }

    /// Public entry point for BRDF LUT compute (debug/testing).
    pub fn brdf_lut_debug(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lut_tex: &wgpu::Texture,
        size: u32,
    ) {
        let shader_src = BRDF_LUT_SHADER.replace("$size", &size.to_string());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("BRDF LUT"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&shader_src)),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba16Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("BRDF LUT"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &lut_tex.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            }],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            cpass.set_pipeline(&pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(size / 8, size / 8, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}
