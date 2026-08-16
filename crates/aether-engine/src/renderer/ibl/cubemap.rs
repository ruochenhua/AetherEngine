// Shared cubemap resources and helpers for IBL generation.

use wgpu::util::DeviceExt;

/// CPU-side cubemap utilities: render-to-cubemap and compute shaders.
pub struct CpuCubemap;

impl CpuCubemap {
    pub(super) fn bgl_pair(
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

    pub(super) fn create_pipeline(
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
pub(super) struct CubeMesh {
    pub(super) vertex_buf: wgpu::Buffer,
    pub(super) index_buf: wgpu::Buffer,
    pub(super) index_count: u32,
}

impl CubeMesh {
    pub(super) fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
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
