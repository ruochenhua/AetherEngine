// Specular prefiltered environment map pass.

use super::cubemap::{capture_projection, capture_views, CpuCubemap, CubeMesh};
use super::shaders::PREFILTER_SHADER;
use wgpu::util::DeviceExt;

impl CpuCubemap {
    /// Prefilter environment map (one pass per mip level).
    pub(super) fn prefiltration(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        env_view: &wgpu::TextureView,
        output: &wgpu::Texture,
        size: u32,
        mip_count: u32,
        cube_mesh: &CubeMesh,
    ) {
        let proj = capture_projection();
        let proj_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&proj),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let flip_zero = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0u32, 0u32, 0u32, 0u32]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (bgl0, bgl1) = Self::bgl_pair(device, wgpu::TextureViewDimension::Cube);
        let bgl2 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl0), Some(&bgl1), Some(&bgl2)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Prefilter"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PREFILTER_SHADER)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Prefilter"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CubeMesh::vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
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

        let views = capture_views();
        for mip in 0..mip_count {
            let _mip_size = size >> mip;
            let roughness = mip as f32 / (mip_count - 1).max(1) as f32;
            let roughness_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[roughness, 0.0f32, 0.0f32, 0.0f32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let roughness_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl2,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: roughness_buf.as_entire_binding(),
                }],
            });

            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
                let face_view = output.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: face,
                    array_layer_count: Some(1),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                {
                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Prefilter"),
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
                    rp.set_bind_group(2, &roughness_bg, &[]);
                    rp.set_vertex_buffer(0, cube_mesh.vertex_buf.slice(..));
                    rp.set_index_buffer(cube_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..cube_mesh.index_count, 0, 0..1);
                }
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }
}
