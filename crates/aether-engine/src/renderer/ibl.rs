//! Image-Based Lighting loader.
//!
//! Follows LearnOpenGL PBR/IBL tutorials:
//! - Diffuse irradiance: https://learnopengl.com/PBR/IBL/Diffuse-irradiance
//! - Specular IBL: https://learnopengl.com/PBR/IBL/Specular-IBL
//!
//! Uses render-to-cubemap (fragment shader) for equirect→cubemap,
//! irradiance convolution, and prefiltering. BRDF LUT uses compute shader.

use std::path::Path;
use wgpu::util::DeviceExt;

/// Configuration for IBL precomputation.
pub struct IblConfig {
    /// Environment cubemap size per face (default: 512).
    pub env_size: u32,
    /// Irradiance cubemap size per face (default: 32).
    pub irradiance_size: u32,
    /// Prefiltered cubemap base size per face (default: 128).
    pub prefilter_size: u32,
    /// Number of mip levels for prefiltered cubemap (default: 5).
    pub prefilter_mips: u32,
    /// BRDF LUT size (default: 256).
    pub brdf_lut_size: u32,
    /// Path to HDR environment map.
    pub environment_path: Option<String>,
}

impl Default for IblConfig {
    fn default() -> Self {
        Self {
            env_size: 512,
            irradiance_size: 32,
            prefilter_size: 128,
            prefilter_mips: 5,
            brdf_lut_size: 256,
            environment_path: None,
        }
    }
}

/// Precomputed IBL resources.
pub struct IblResources {
    /// Diffuse irradiance cubemap (32×32, Rgba16Float).
    pub irradiance_view: wgpu::TextureView,
    /// Prefiltered specular cubemap (128×128, 5 mips, Rgba16Float).
    pub prefiltered_view: wgpu::TextureView,
    /// BRDF integration LUT (256×256, Rgba16Float, RG channels).
    pub brdf_lut_view: wgpu::TextureView,
    /// Shared sampler (trilinear, clamp-to-edge) for all IBL textures.
    pub ibl_sampler: wgpu::Sampler,
    _irradiance_texture: wgpu::Texture,
    _prefiltered_texture: wgpu::Texture,
    _brdf_lut_texture: wgpu::Texture,
}

impl IblResources {
    /// Generate all IBL resources. Pass `None` for queue in tests.
    pub fn generate(
        device: &wgpu::Device,
        queue: Option<&wgpu::Queue>,
        config: &IblConfig,
    ) -> Self {
        let ibl_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IBL Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let env_tex = create_cubemap(device, config.env_size, 1, "Env");
        let irradiance_tex = create_cubemap(device, config.irradiance_size, 1, "Irr");
        let prefiltered_tex =
            create_cubemap(device, config.prefilter_size, config.prefilter_mips, "Pref");
        let brdf_lut_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("BRDF LUT"),
            size: wgpu::Extent3d {
                width: config.brdf_lut_size,
                height: config.brdf_lut_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        let env_view = env_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let irradiance_view = irradiance_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let prefiltered_view = prefiltered_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let brdf_lut_view = brdf_lut_tex.create_view(&wgpu::TextureViewDescriptor::default());

        if let Some(queue) = queue {
            let (hdr_tex, hdr_view, hdr_sampler) = load_hdr_texture(device, queue, config);
            let cube_mesh = CubeMesh::new(device);

            // 1. Equirect → Cubemap
            CpuCubemap::equirect_to_cubemap(
                device, queue, &hdr_view, &hdr_sampler, &env_tex, config.env_size, &cube_mesh,
            );

            // 2. Irradiance convolution
            CpuCubemap::irradiance_convolution(
                device, queue, &env_view, &irradiance_tex, config.irradiance_size, &cube_mesh,
            );

            // 3. Prefilter (one pass per mip)
            CpuCubemap::prefiltration(
                device, queue, &env_view, &prefiltered_tex, config.prefilter_size, config.prefilter_mips, &cube_mesh,
            );

            // 4. BRDF LUT (compute)
            CpuCubemap::brdf_integration(device, queue, &brdf_lut_tex, config.brdf_lut_size);

            drop((hdr_tex, hdr_view, hdr_sampler));
        }

        Self {
            irradiance_view,
            prefiltered_view,
            brdf_lut_view,
            ibl_sampler,
            _irradiance_texture: irradiance_tex,
            _prefiltered_texture: prefiltered_tex,
            _brdf_lut_texture: brdf_lut_tex,
        }
    }
}

// ── HDR Loading ──────────────────────────────────────────────────────

fn load_hdr_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: &IblConfig,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let path = config
        .environment_path
        .as_deref()
        .unwrap_or("assets/hdr/newport_loft.hdr");
    let img = image::open(Path::new(path))
        .unwrap_or_else(|e| panic!("Failed to load HDR '{}': {}", path, e))
        .to_rgb32f();

    let (w, h) = (img.width(), img.height());
    let mut rgba: Vec<f32> = Vec::with_capacity((w * h * 4) as usize);
    for p in img.pixels() {
        rgba.push(p.0[0]);
        rgba.push(p.0[1]);
        rgba.push(p.0[2]);
        rgba.push(1.0);
    }

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
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&rgba),
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(16 * w),
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

struct CubeMesh {
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

    fn new(device: &wgpu::Device) -> Self {
        #[rustfmt::skip]
        let vertices: [f32; 108] = [
            -1.0,-1.0, 1.0,  1.0,-1.0, 1.0,  1.0, 1.0, 1.0,  -1.0,-1.0, 1.0,  1.0, 1.0, 1.0, -1.0, 1.0, 1.0,
            -1.0,-1.0,-1.0, -1.0, 1.0,-1.0,  1.0, 1.0,-1.0,  -1.0,-1.0,-1.0,  1.0, 1.0,-1.0,  1.0,-1.0,-1.0,
            -1.0, 1.0,-1.0, -1.0, 1.0, 1.0,  1.0, 1.0, 1.0,  -1.0, 1.0,-1.0,  1.0, 1.0, 1.0,  1.0, 1.0,-1.0,
            -1.0,-1.0,-1.0,  1.0,-1.0,-1.0,  1.0,-1.0, 1.0,  -1.0,-1.0,-1.0,  1.0,-1.0, 1.0, -1.0,-1.0, 1.0,
            -1.0,-1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0,-1.0,  -1.0,-1.0, 1.0, -1.0, 1.0,-1.0, -1.0,-1.0,-1.0,
             1.0,-1.0, 1.0,  1.0,-1.0,-1.0,  1.0, 1.0,-1.0,   1.0,-1.0, 1.0,  1.0, 1.0,-1.0,  1.0, 1.0, 1.0,
        ];
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube VB"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices: [u32; 36] =
            std::array::from_fn(|i| i as u32);
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

fn capture_views() -> [[f32; 16]; 6] {
    let look_at = |eye: [f32; 3], center: [f32; 3], up: [f32; 3]| {
        glam::Mat4::look_at_rh(
            glam::Vec3::from_array(eye),
            glam::Vec3::from_array(center),
            glam::Vec3::from_array(up),
        )
        .to_cols_array()
    };
    [
        look_at([0.,0.,0.], [ 1.,0.,0.], [0.,-1.,0.]),  // +X
        look_at([0.,0.,0.], [-1.,0.,0.], [0.,-1.,0.]),  // -X
        look_at([0.,0.,0.], [0., 1.,0.], [0.,0., 1.]),  // +Y
        look_at([0.,0.,0.], [0.,-1.,0.], [0.,0.,-1.]),  // -Y
        look_at([0.,0.,0.], [0.,0., 1.], [0.,-1.,0.]),  // +Z
        look_at([0.,0.,0.], [0.,0.,-1.], [0.,-1.,0.]),  // -Z
    ]
}

fn capture_projection() -> [f32; 16] {
    glam::Mat4::perspective_rh(90.0f32.to_radians(), 1.0, 0.1, 10.0).to_cols_array()
}

// ── Render-to-cubemap logic ──────────────────────────────────────────

struct CpuCubemap;

impl CpuCubemap {
    /// Equirectangular → Cubemap
    fn equirect_to_cubemap(
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
        let pipeline = Self::create_pipeline(
            device, EQUIRECT_SHADER, &bgl0, &bgl1, "Equirect",
        );

        let views = capture_views();
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
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
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
    fn irradiance_convolution(
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
        let pipeline = Self::create_pipeline(
            device, IRRADIANCE_SHADER, &bgl0, &bgl1, "Irradiance",
        );
        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let views = capture_views();
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
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
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
    fn prefiltration(
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
            bind_group_layouts: &[&bgl0, &bgl1, &bgl2],
            push_constant_ranges: &[],
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
                entry_point: "vs_main",
                buffers: &[CubeMesh::vertex_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
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
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
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
    fn brdf_integration(
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
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("BRDF LUT"),
            layout: Some(&layout),
            module: &shader,
            entry_point: "main",
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
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut cpass =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            cpass.set_pipeline(&pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(size / 8, size / 8, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    // Helpers

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
            bind_group_layouts: &[bgl0, bgl1],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[CubeMesh::vertex_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        })
    }
}

// ── WGSL Shaders ─────────────────────────────────────────────────────

const EQUIRECT_SHADER: &str = r#"
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

const invAtan: vec2<f32> = vec2<f32>(0.1591, 0.3183);
fn sample_spherical_map(v: vec3<f32>) -> vec2<f32> {
    var uv = vec2<f32>(atan2(v.z, v.x), asin(v.y));
    uv = uv * invAtan;
    uv = uv + vec2<f32>(0.5);
    return uv;
}

@group(1) @binding(0) var equirect_map: texture_2d<f32>;
@group(1) @binding(1) var equirect_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.local_pos);
    let uv = sample_spherical_map(dir);
    return textureSample(equirect_map, equirect_sampler, uv);
}
"#;

const IRRADIANCE_SHADER: &str = r#"
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

const PREFILTER_SHADER: &str = r#"
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

struct Roughness { roughness: f32, _pad: vec3<u32>, };

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

const BRDF_LUT_SHADER: &str = r#"
@group(0) @binding(0) var output_lut: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 1024u;

fn radical_inverse_vdc(mut bits: u32) -> f32 {
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
    let a = roughness * roughness;
    let k = a * a / 2.0;
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

fn create_cubemap(device: &wgpu::Device, size: u32, mips: u32, label: &str) -> wgpu::Texture {
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
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device_and_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .expect("need device")
    }

    #[test]
    fn ibl_resources_created_with_correct_sizes() {
        let (device, _queue) = headless_device_and_queue();
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, None, &config);
        assert_eq!(ibl._irradiance_texture.size().width, 32);
        assert_eq!(ibl._irradiance_texture.depth_or_array_layers(), 6);
        assert_eq!(ibl._prefiltered_texture.mip_level_count(), 5);
        assert_eq!(ibl._brdf_lut_texture.size().width, 256);
    }

    #[test]
    fn ibl_texture_formats_are_correct() {
        let (device, _queue) = headless_device_and_queue();
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, None, &config);
        assert_eq!(
            ibl._irradiance_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            ibl._prefiltered_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            ibl._brdf_lut_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
    }
}
