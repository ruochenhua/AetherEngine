//! SSR pipeline and device-object creation.
//!
//! Builds the trace and upsample render pipelines, bind group layouts,
//! uniform buffer, full-screen quad geometry, and samplers used by [`SSRPass`].

use super::types::SSRSettings;
use std::borrow::Cow;
use std::mem::size_of;
use wgpu::util::DeviceExt;
#[cfg(test)]
mod shader_tests;
mod trace_shader;
mod upsample_shader;
/// Device-owned objects required by [`SSRPass`].
pub(super) struct DeviceObjects {
    /// Half-resolution ray-march pipeline.
    pub(super) trace_pipeline: wgpu::RenderPipeline,
    /// Full-resolution bilateral upsample pipeline.
    pub(super) upsample_pipeline: wgpu::RenderPipeline,
    /// Full-screen quad vertex buffer.
    pub(super) quad_vertex_buffer: wgpu::Buffer,
    /// Number of vertices in the full-screen quad.
    pub(super) quad_vertex_count: u32,
    /// Uniform buffer holding the current [`SSRSettings`].
    pub(super) settings_buffer: wgpu::Buffer,
    /// Bind group for the uniform buffer.
    pub(super) settings_bind_group: wgpu::BindGroup,
    /// Layout for the uniform bind group (kept for rebuilds).
    #[allow(dead_code)]
    pub(super) settings_bind_group_layout: wgpu::BindGroupLayout,
    /// Layout shared by the trace and upsample texture bind groups.
    pub(super) texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Linear sampler used for texture reads.
    pub(super) sampler: wgpu::Sampler,
    /// 1x1 dummy texture view bound at slot 6 during the trace pass.
    pub(super) dummy_texture_view: wgpu::TextureView,
}

/// Create all device-owned objects for the SSR pass.
pub(super) fn create_device_objects(device: &wgpu::Device) -> DeviceObjects {
    let shader_source = trace_shader::SSR_TRACE_SHADER_SRC;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SSR Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });

    let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SSR Texture BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });

    let settings_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SSR Settings BGL"),
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

    // Shared pipeline layout (both trace and upsample use the same bind groups)
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("SSR Pipeline Layout"),
        bind_group_layouts: &[Some(&texture_bgl), Some(&settings_bgl)],
        immediate_size: 0,
    });

    let trace_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SSR Trace (Half-Res)"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<[f32; 2]>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                }],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // ── Upsample shader + pipeline ──────────────────────────
    let upsample_source = upsample_shader::SSR_UPSAMPLE_SHADER_SRC;
    let upsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SSR Upsample Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(upsample_source)),
    });
    let upsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SSR Upsample"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &upsample_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<[f32; 2]>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                }],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &upsample_shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let settings_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SSR Settings Buffer"),
        size: size_of::<SSRSettings>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let settings_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SSR Settings BG"),
        layout: &settings_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: settings_buffer.as_entire_binding(),
        }],
    });

    let quad_vertices: [[f32; 2]; 6] = [
        [-1.0, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
    ];
    let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SSR Quad Vtx"),
        contents: bytemuck::cast_slice(&quad_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("SSR Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SSR Dummy 1x1"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let dummy_texture_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());

    DeviceObjects {
        trace_pipeline,
        upsample_pipeline,
        quad_vertex_buffer,
        quad_vertex_count: 6,
        settings_buffer,
        settings_bind_group,
        settings_bind_group_layout: settings_bgl,
        texture_bind_group_layout: texture_bgl,
        sampler,
        dummy_texture_view,
    }
}
