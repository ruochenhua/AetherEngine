// Diffuse irradiance convolution pass.

use super::cubemap::{capture_projection, capture_views, CpuCubemap, CubeMesh};
use super::shaders::IRRADIANCE_SHADER;
use wgpu::util::DeviceExt;

impl CpuCubemap {
    /// Irradiance convolution on environment cubemap.
    pub(super) fn irradiance_convolution(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        env_view: &wgpu::TextureView,
        output: &wgpu::Texture,
        _size: u32,
        cube_mesh: &CubeMesh,
    ) {
        let proj = capture_projection();
        let proj_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&proj),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (bgl0, bgl1) = Self::bgl_pair(device, wgpu::TextureViewDimension::Cube);
        let pipeline = Self::create_pipeline(device, IRRADIANCE_SHADER, &bgl0, &bgl1, "Irradiance");
        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let flip_zero = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0u32, 0u32, 0u32, 0u32]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let views = capture_views();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for face in 0u32..6 {
            let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&views[face as usize]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: proj_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: view_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: flip_zero.as_entire_binding(),
                    },
                ],
            });
            let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(env_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&env_sampler),
                    },
                ],
            });
            let face_view = output.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                ..Default::default()
            });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Irradiance"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &face_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                rp.set_pipeline(&pipeline);
                rp.set_bind_group(0, &vp_bg, &[]);
                rp.set_bind_group(1, &tex_bg, &[]);
                rp.set_vertex_buffer(0, cube_mesh.vertex_buf.slice(..));
                rp.set_index_buffer(cube_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..cube_mesh.index_count, 0, 0..1);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}
