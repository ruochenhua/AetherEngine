// Equirectangular HDR to cubemap pass.

use super::cubemap::{capture_projection, capture_views, CpuCubemap, CubeMesh};
use super::shaders::EQUIRECT_SHADER;
use wgpu::util::DeviceExt;

impl CpuCubemap {
    /// Equirectangular → Cubemap
    pub(super) fn equirect_to_cubemap(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hdr_view: &wgpu::TextureView,
        hdr_sampler: &wgpu::Sampler,
        cubemap: &wgpu::Texture,
        _size: u32,
        cube_mesh: &CubeMesh,
    ) {
        let proj = capture_projection();
        let proj_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&proj),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (bgl0, bgl1) = Self::bgl_pair(device, wgpu::TextureViewDimension::D2);
        let pipeline = Self::create_pipeline(device, EQUIRECT_SHADER, &bgl0, &bgl1, "Equirect");

        let views = capture_views();
        let flips: [[u32; 4]; 6] = [
            [0, 0, 0, 0], // +X
            [0, 0, 0, 0], // -X
            [0, 0, 0, 0], // -Y view
            [0, 0, 0, 0], // +Y view
            [0, 0, 0, 0], // +Z
            [0, 0, 0, 0], // -Z
        ];
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for face in 0u32..6 {
            let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&views[face as usize]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let flip_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&flips[face as usize]),
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
                        resource: flip_buf.as_entire_binding(),
                    },
                ],
            });
            let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(hdr_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(hdr_sampler),
                    },
                ],
            });
            let face_view = cubemap.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                ..Default::default()
            });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Eq2Cube"),
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
