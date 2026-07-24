//! Cubemap generation helpers for IBL.

use std::path::Path;

use wgpu::util::DeviceExt;

use super::config::IblConfig;

/// CPU-side cubemap utilities: render-to-cubemap and compute shaders.
pub struct CpuCubemap;

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

    fn bgl_pair(
        device: &wgpu::Device,
        tex_dim: wgpu::TextureViewDimension,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroupLayout) {
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: tex_dim,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        (bgl0, bgl1)
    }

    fn create_pipeline(
        device: &wgpu::Device,
        shader_src: &str,
        bgl0: &wgpu::BindGroupLayout,
        bgl1: &wgpu::BindGroupLayout,
        label: &str,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_src)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(bgl0), Some(bgl1)],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
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
        })
    }
}

// ── HDR Loading ──────────────────────────────────────────────────────

pub(super) fn load_hdr_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: &IblConfig,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let (w, h, rgba) = if config.debug_checkerboard {
        // Generate a 16×8 magenta/cyan checkerboard for debugging
        let w = 16u32;
        let h = 8u32;
        let mut data: Vec<u8> = Vec::with_capacity((w * h * 8) as usize); // Rgba16Float = 8 bytes/pixel
        for y in 0..h {
            for x in 0..w {
                let is_magenta = ((x / 2) + (y / 2)) % 2 == 0;
                let (r, g, b) = if is_magenta {
                    (1.0f32, 0.0, 1.0)
                } else {
                    (0.0f32, 1.0, 1.0)
                };
                for c in [r, g, b, 1.0f32] {
                    data.extend_from_slice(&half::f16::from_f32(c).to_bits().to_le_bytes());
                }
            }
        }
        (w, h, data)
    } else {
        let path = config
            .environment_path
            .as_deref()
            .unwrap_or("assets/hdr/newport_loft.hdr");
        let img = image::open(Path::new(path))
            .unwrap_or_else(|e| panic!("Failed to load HDR '{}': {}", path, e))
            .to_rgb32f();

        let (iw, ih) = (img.width(), img.height());
        let mut data2: Vec<u8> = Vec::with_capacity((iw * ih * 8) as usize);
        for p in img.pixels() {
            for c in 0..3 {
                data2.extend_from_slice(&half::f16::from_f32(p.0[c]).to_bits().to_le_bytes());
            }
            data2.extend_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
        }
        (iw, ih, data2)
    };

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8 * w), // Rgba16Float = 8 bytes/pixel
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );

    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("HDR Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (tex, view, sampler)
}

// ── Cube mesh ────────────────────────────────────────────────────────

pub(super) struct CubeMesh {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
}

impl CubeMesh {
    fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        }
    }

    pub(super) fn new(device: &wgpu::Device) -> Self {
        // Unit cube vertices: one face at a time, 2 triangles (6 verts) per face.
        // Cube faces render from INSIDE the cube (camera at origin).
        #[rustfmt::skip]
        let vertices: [f32; 108] = [
            // +X
             1.0,  1.0,  1.0,  1.0, -1.0,  1.0,  1.0, -1.0, -1.0,
             1.0,  1.0,  1.0,  1.0, -1.0, -1.0,  1.0,  1.0, -1.0,
            // -X
            -1.0,  1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0,  1.0,
            -1.0,  1.0, -1.0, -1.0, -1.0,  1.0, -1.0,  1.0,  1.0,
            // +Y
            -1.0,  1.0,  1.0,  1.0,  1.0,  1.0,  1.0,  1.0, -1.0,
            -1.0,  1.0,  1.0,  1.0,  1.0, -1.0, -1.0,  1.0, -1.0,
            // -Y
            -1.0, -1.0, -1.0,  1.0, -1.0, -1.0,  1.0, -1.0,  1.0,
            -1.0, -1.0, -1.0,  1.0, -1.0,  1.0, -1.0, -1.0,  1.0,
            // +Z
             1.0,  1.0,  1.0, -1.0,  1.0,  1.0, -1.0, -1.0,  1.0,
             1.0,  1.0,  1.0, -1.0, -1.0,  1.0,  1.0, -1.0,  1.0,
            // -Z
             1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0,  1.0, -1.0,
             1.0, -1.0, -1.0, -1.0,  1.0, -1.0,  1.0,  1.0, -1.0,
        ];
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube VB"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices: [u32; 36] = std::array::from_fn(|i| i as u32);
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube IB"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buf,
            index_buf,
            index_count: 36,
        }
    }
}

// ── Capture views (same as LearnOpenGL) ──────────────────────────────

pub(super) fn capture_views() -> [[f32; 16]; 6] {
    let look_at = |eye: [f32; 3], center: [f32; 3], up: [f32; 3]| {
        glam::Mat4::look_at_rh(
            glam::Vec3::from_array(eye),
            glam::Vec3::from_array(center),
            glam::Vec3::from_array(up),
        )
        .to_cols_array()
    };
    [
        look_at([0., 0., 0.], [1., 0., 0.], [0., -1., 0.]), // +X
        look_at([0., 0., 0.], [-1., 0., 0.], [0., -1., 0.]), // -X
        look_at([0., 0., 0.], [0., -1., 0.], [0., 0., -1.]), // +Y layer ← render -Y view
        look_at([0., 0., 0.], [0., 1., 0.], [0., 0., 1.]),  // -Y layer ← render +Y view
        look_at([0., 0., 0.], [0., 0., 1.], [0., -1., 0.]), // +Z
        look_at([0., 0., 0.], [0., 0., -1.], [0., -1., 0.]), // -Z
    ]
}

pub(super) fn capture_projection() -> [f32; 16] {
    // glam::perspective_rh outputs OpenGL z∈[-1,1]. wgpu expects z∈[0,1].
    // Correction: z_wgpu_ndc = (z_gl_ndc + 1) / 2
    //   z' = z_gl + w_gl,  w' = 2*w_gl    (maps z to [0,1])
    //   x' = 2*x_gl,      y' = 2*y_gl    (compensate to keep x/w, y/w unchanged)
    let p_gl = glam::Mat4::perspective_rh(90.0f32.to_radians(), 1.0, 0.1, 10.0);
    let correction = glam::Mat4::from_cols_array(&[
        2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 2.0,
    ]);
    // correction * p_gl: apply GL projection first, then z-correction
    (correction * p_gl).to_cols_array()
}

// ── WGSL Shaders ─────────────────────────────────────────────────────

pub(crate) const EQUIRECT_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };
struct Flip { flip_x: u32, flip_y: u32, flip_z: u32, _pad: u32, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const invAtan: vec2<f32> = vec2<f32>(0.1591, 0.3183);
fn sample_spherical_map(v: vec3<f32>, flip_x: u32, flip_y: u32, flip_z: u32) -> vec2<f32> {
    let vx = select(v.x, -v.x, flip_x == 1u);
    let vy = select(v.y, -v.y, flip_y == 1u);
    let vz = select(v.z, -v.z, flip_z == 1u);
    var uv = vec2<f32>(atan2(vz, vx), asin(vy));
    uv = uv * invAtan;
    uv = uv + vec2<f32>(0.5);
    return uv;
}

@group(0) @binding(2) var<uniform> flip: Flip;

@group(1) @binding(0) var equirect_map: texture_2d<f32>;
@group(1) @binding(1) var equirect_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.local_pos);
    let uv = sample_spherical_map(dir, flip.flip_x, flip.flip_y, flip.flip_z);
    return textureSample(equirect_map, equirect_sampler, uv);
}
"#;

pub(crate) const IRRADIANCE_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const PI: f32 = 3.14159265359;
const sample_delta: f32 = 0.025;

@group(1) @binding(0) var environment_map: texture_cube<f32>;
@group(1) @binding(1) var env_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.local_pos);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(N.y) > 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let right = normalize(cross(up, N));
    up = cross(N, right);

    var irradiance = vec3<f32>(0.0);
    var nr_samples: f32 = 0.0;

    var phi: f32 = 0.0;
    while (phi < 2.0 * PI) {
        var theta: f32 = 0.0;
        while (theta < 0.5 * PI) {
            let sin_theta = sin(theta);
            let tangent_sample = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos(theta));
            let sample_vec = tangent_sample.x * right + tangent_sample.y * up + tangent_sample.z * N;
            irradiance += textureSample(environment_map, env_sampler, sample_vec).rgb * cos(theta) * sin_theta;
            nr_samples += 1.0;
            theta += sample_delta;
        }
        phi += sample_delta;
    }
    irradiance = PI * irradiance / nr_samples;
    return vec4<f32>(irradiance, 1.0);
}
"#;

pub(crate) const PREFILTER_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 1024u;

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}
fn hammersley(i: u32, N: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(N), radical_inverse_vdc(i));
}
fn importance_sample_ggx(Xi: vec2<f32>, N: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * Xi.x;
    let cos_theta = sqrt((1.0 - Xi.y) / (1.0 + (a * a - 1.0) * Xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let H = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(N.z) < 0.999);
    let tangent = normalize(cross(up, N));
    let bitangent = cross(N, tangent);
    return normalize(tangent * H.x + bitangent * H.y + N * H.z);
}

struct Roughness { roughness: f32, _pad: f32, _pad2: f32, _pad3: f32, };

@group(1) @binding(0) var environment_map: texture_cube<f32>;
@group(1) @binding(1) var env_sampler: sampler;

@group(2) @binding(0) var<uniform> u_roughness: Roughness;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.local_pos);
    let R = N;
    let V = R;
    let roughness = u_roughness.roughness;

    var prefiltered_color = vec3<f32>(0.0);
    var total_weight: f32 = 0.0;

    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let Xi = hammersley(i, SAMPLE_COUNT);
        let H = importance_sample_ggx(Xi, N, roughness);
        let L = normalize(2.0 * dot(V, H) * H - V);
        let NdotL = max(dot(N, L), 0.0);
        if (NdotL > 0.0) {
            prefiltered_color += textureSample(environment_map, env_sampler, L).rgb * NdotL;
            total_weight += NdotL;
        }
    }
    prefiltered_color = prefiltered_color / total_weight;
    return vec4<f32>(prefiltered_color, 1.0);
}
"#;

pub(crate) const BRDF_LUT_SHADER: &str = r#"
@group(0) @binding(0) var output_lut: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 1024u;

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}
fn hammersley(i: u32, N: u32) -> vec2<f32> { return vec2<f32>(f32(i) / f32(N), radical_inverse_vdc(i)); }
fn importance_sample_ggx(Xi: vec2<f32>, N: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * Xi.x;
    let cos_theta = sqrt((1.0 - Xi.y) / (1.0 + (a * a - 1.0) * Xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let H = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(N.z) < 0.999);
    let tangent = normalize(cross(up, N));
    let bitangent = cross(N, tangent);
    return normalize(tangent * H.x + bitangent * H.y + N * H.z);
}
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;  // Schlick-GGX (matches LearnOpenGL tutorial)
    return (NdotV / (NdotV * (1.0 - k) + k)) * (NdotL / (NdotL * (1.0 - k) + k));
}
fn integrate_brdf(NdotV: f32, roughness: f32) -> vec2<f32> {
    let V = vec3<f32>(sqrt(1.0 - NdotV * NdotV), 0.0, NdotV);
    let N = vec3<f32>(0.0, 0.0, 1.0);
    var A: f32 = 0.0;
    var B: f32 = 0.0;
    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let Xi = hammersley(i, SAMPLE_COUNT);
        let H = importance_sample_ggx(Xi, N, roughness);
        let L = normalize(2.0 * dot(V, H) * H - V);
        let NdotL = max(L.z, 0.0);
        let NdotH = max(H.z, 0.0);
        let VdotH = max(dot(V, H), 0.0);
        if (NdotL > 0.0) {
            let G = geometry_smith(NdotV, NdotL, roughness);
            let G_vis = (G * VdotH) / (NdotH * NdotV);
            let Fc = pow(1.0 - VdotH, 5.0);
            A += (1.0 - Fc) * G_vis;
            B += Fc * G_vis;
        }
    }
    return vec2<f32>(A, B) / f32(SAMPLE_COUNT);
}
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= $size || id.y >= $size) { return; }
    let NdotV = (f32(id.x) + 0.5) / f32($size);
    let roughness = (f32(id.y) + 0.5) / f32($size);
    let result = integrate_brdf(NdotV, roughness);
    textureStore(output_lut, vec2<i32>(id.xy), vec4<f32>(result, 0.0, 1.0));
}
"#;

// ── Cubemap helpers ──────────────────────────────────────────────────

pub(super) fn create_cubemap(
    device: &wgpu::Device,
    size: u32,
    mips: u32,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{IblConfig, IblResources};
    use super::{load_hdr_texture, CpuCubemap, CubeMesh};
    use crate::test_utils::headless_device_queue;

    /// Tiny sizes + procedural 16×8 checkerboard input so no HDR file is read.
    fn tiny_checkerboard_config() -> IblConfig {
        IblConfig {
            env_size: 64,
            irradiance_size: 16,
            prefilter_size: 32,
            prefilter_mips: 3,
            brdf_lut_size: 64,
            environment_path: None,
            debug_checkerboard: true,
        }
    }

    /// Copy an Rgba16Float texture (one or more array layers) back to CPU memory.
    /// `width * 8` must be a multiple of 256 (wgpu copy row alignment).
    fn read_texture_pixels(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        layers: u32,
    ) -> Vec<u8> {
        let bytes_per_row = width * 8; // Rgba16Float = 8 bytes/pixel
        assert_eq!(
            bytes_per_row % 256,
            0,
            "test texture width breaks copy alignment"
        );
        let layer_bytes = (bytes_per_row * height) as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Readback"),
            size: layer_bytes * layers as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for layer in 0..layers {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: layer_bytes * layer as u64,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        slice.get_mapped_range().to_vec()
    }

    fn decode_rgba16f(data: &[u8]) -> impl Iterator<Item = [f32; 4]> + '_ {
        data.chunks_exact(8).map(|px| {
            [
                half::f16::from_bits(u16::from_le_bytes([px[0], px[1]])).to_f32(),
                half::f16::from_bits(u16::from_le_bytes([px[2], px[3]])).to_f32(),
                half::f16::from_bits(u16::from_le_bytes([px[4], px[5]])).to_f32(),
                half::f16::from_bits(u16::from_le_bytes([px[6], px[7]])).to_f32(),
            ]
        })
    }

    #[test]
    fn equirect_to_cubemap_renders_checkerboard_colors() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let config = tiny_checkerboard_config();
        let (_hdr_tex, hdr_view, hdr_sampler) = load_hdr_texture(&device, &queue, &config);

        // Test-owned cubemap with COPY_SRC so pixels can be read back;
        // create_cubemap itself only requests TEXTURE_BINDING | RENDER_ATTACHMENT | COPY_DST.
        let cubemap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Cubemap"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cube_mesh = CubeMesh::new(&device);
        CpuCubemap::equirect_to_cubemap(
            &device,
            &queue,
            &hdr_view,
            &hdr_sampler,
            &cubemap,
            64,
            &cube_mesh,
        );

        let pixels = read_texture_pixels(&device, &queue, &cubemap, 64, 64, 6);
        let (mut magenta, mut cyan) = (0usize, 0usize);
        for (i, [r, g, b, _a]) in decode_rgba16f(&pixels).enumerate() {
            assert!(
                r.is_finite() && g.is_finite() && b.is_finite(),
                "cubemap pixel {i} is non-finite: [{r}, {g}, {b}]"
            );
            if r > 0.8 && b > 0.8 && g < 0.2 {
                magenta += 1;
            }
            if g > 0.8 && b > 0.8 && r < 0.2 {
                cyan += 1;
            }
        }
        assert!(
            magenta > 0,
            "equirect→cubemap produced no magenta checkerboard pixels"
        );
        assert!(
            cyan > 0,
            "equirect→cubemap produced no cyan checkerboard pixels"
        );
    }

    #[test]
    fn ibl_generate_with_queue_runs_all_pipelines() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let config = tiny_checkerboard_config();
        // First test to execute all four generation pipelines; must not panic.
        let ibl = IblResources::generate(&device, Some(&queue), &config);

        let irr = ibl.irradiance_texture();
        assert_eq!(irr.size().width, 16, "irradiance cubemap width");
        assert_eq!(irr.size().height, 16, "irradiance cubemap height");
        assert_eq!(
            irr.depth_or_array_layers(),
            6,
            "irradiance cubemap layer count"
        );
        assert_eq!(irr.mip_level_count(), 1, "irradiance cubemap mip count");
        assert_eq!(
            irr.format(),
            wgpu::TextureFormat::Rgba16Float,
            "irradiance cubemap format"
        );

        let pref = ibl.prefiltered_texture();
        assert_eq!(pref.size().width, 32, "prefiltered cubemap width");
        assert_eq!(
            pref.depth_or_array_layers(),
            6,
            "prefiltered cubemap layer count"
        );
        assert_eq!(pref.mip_level_count(), 3, "prefiltered cubemap mip count");
        assert_eq!(
            pref.format(),
            wgpu::TextureFormat::Rgba16Float,
            "prefiltered cubemap format"
        );

        let lut = ibl.brdf_lut_texture();
        assert_eq!(lut.size().width, 64, "BRDF LUT width");
        assert_eq!(lut.size().height, 64, "BRDF LUT height");
        assert_eq!(lut.depth_or_array_layers(), 1, "BRDF LUT layer count");
        assert_eq!(
            lut.format(),
            wgpu::TextureFormat::Rgba16Float,
            "BRDF LUT format"
        );
    }

    #[test]
    fn brdf_lut_compute_writes_valid_values() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let size = 64u32;
        // Test-owned LUT texture with COPY_SRC for readback; the production
        // texture omits COPY_SRC, and brdf_lut_debug only needs STORAGE_BINDING.
        let lut = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test BRDF LUT"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        CpuCubemap::brdf_lut_debug(&device, &queue, &lut, size);

        let pixels = read_texture_pixels(&device, &queue, &lut, size, size, 1);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut nonzero = 0usize;
        for (i, [r, g, b, a]) in decode_rgba16f(&pixels).enumerate() {
            assert!(
                r.is_finite() && g.is_finite(),
                "BRDF LUT texel {i} is non-finite: [{r}, {g}]"
            );
            assert_eq!(b, 0.0, "BRDF LUT texel {i} blue channel should be 0");
            assert_eq!(a, 1.0, "BRDF LUT texel {i} alpha should be 1");
            for v in [r, g] {
                min = min.min(v);
                max = max.max(v);
                if v != 0.0 {
                    nonzero += 1;
                }
            }
        }
        assert!(
            min >= 0.0 && max <= 1.0,
            "BRDF LUT values outside [0,1]: min={min}, max={max}"
        );
        assert!(nonzero > 0, "BRDF LUT is entirely zero");
    }
}
