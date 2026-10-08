//! Shadow pipeline construction and public pass accessors.

use super::*;

impl ShadowPass {
    /// Create a new cascaded shadow pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let src = shaders::SHADOW_SHADER_SRC;
        let shader = ShaderLibrary::create_shader_module(device, "Shadow Shader", src)
            .expect("Shadow shader must be valid WGSL");

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("S VP BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow PL"),
            bind_group_layouts: &[Some(&vp_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), InstanceData::instance_desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let aligned_size = Self::aligned_cascade_uniform_size();
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S VP"),
            size: (CASCADE_COUNT as u64) * aligned_size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S VP BG"),
            layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &vp_buf,
                    offset: 0,
                    size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<CascadeUniform>() as u64)
                            .unwrap(),
                    ),
                }),
            }],
        });

        let initial_instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Instance Buf"),
            size: (initial_instance_capacity * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            cascade_uniform_buffer: vp_buf,
            cascade_bind_group: vp_bg,
            instance_buffer,
            instance_buffer_capacity: initial_instance_capacity,
            shadow_depth_handle: None,
            cascade_views: Vec::new(),
            batches: Arc::from([]),
            animation_bounds: Arc::from([]),
            cascades: [Cascade {
                view_proj: Mat4::IDENTITY,
                split_depth: 0.0,
            }; CASCADE_COUNT],
            terrain_geometry: None,
        }
    }

    /// Return the CPU-side cascade data computed for the current frame.
    pub fn cascades(&self) -> &[Cascade; CASCADE_COUNT] {
        &self.cascades
    }

    pub(super) fn aligned_cascade_uniform_size() -> usize {
        let size = std::mem::size_of::<CascadeUniform>();
        let alignment = 256; // wgpu uniform dynamic offset alignment
        size.div_ceil(alignment) * alignment
    }
}
