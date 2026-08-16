// Water reflection pipeline construction.

use super::{ReflectionUniform, WaterReflectionPass};
use crate::asset::mesh::{InstanceData, Vertex};
use crate::asset::texture::GpuTexture;
use crate::renderer::renderable::ObjectUniform;
use crate::terrain::{
    create_terrain_material_bind_group, create_terrain_material_bind_group_layout,
    ChunkInstanceData, TerrainUniform,
};
use glam::{Mat4, Vec3};
use std::sync::Arc;

impl WaterReflectionPass {
    /// Create a new planar reflection pass.
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue) -> Self {
        let shader_source = super::shaders::REFLECTION_SHADER_SRC;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WaterReflection Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("WaterReflection Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let obj_size = std::mem::size_of::<ObjectUniform>() as u64;
        let object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("WaterReflection Obj BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("WaterReflection Texture BGL"),
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("WaterReflection Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&object_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("WaterReflection Pipeline"),
            layout: Some(&pipeline_layout),
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
                // Reflection flips triangle winding, so cull the opposite faces.
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Terrain reflection shader: splatted albedo with simple forward lighting.
        let terrain_shader_source = super::shaders::TERRAIN_REFLECTION_SHADER_SRC;
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WaterReflection Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(terrain_shader_source)),
        });

        let terrain_bind_group_layout = create_terrain_material_bind_group_layout(device);
        let terrain_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("WaterReflection Terrain Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&uniform_bind_group_layout),
                    Some(&terrain_bind_group_layout),
                ],
                immediate_size: 0,
            });

        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("WaterReflection Terrain Pipeline"),
            layout: Some(&terrain_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), ChunkInstanceData::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
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
                // Reflection flips triangle winding, so cull the opposite faces.
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let terrain_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Terrain Material Buf"),
            size: std::mem::size_of::<TerrainUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let fallback_white = Arc::new(GpuTexture::from_cpu(
            device,
            _queue,
            &crate::asset::texture::CpuTexture::from_color(255, 255, 255, 255),
            Some("water_reflection_terrain_fallback_white"),
        ));
        let terrain_bind_group = create_terrain_material_bind_group(
            device,
            &terrain_bind_group_layout,
            &terrain_buffer,
            &fallback_white,
            &fallback_white,
            &fallback_white,
            &fallback_white,
            &fallback_white,
        );

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Uniform Buffer"),
            size: std::mem::size_of::<ReflectionUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WaterReflection Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let object_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Object Buffer"),
            size: 256 * obj_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WaterReflection Object Bind Group"),
            layout: &object_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &object_buffer,
                    offset: 0,
                    size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                }),
            }],
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Instance Buffer"),
            size: (256 * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            object_buffer,
            object_buffer_capacity: 256,
            object_bind_group,
            texture_bind_group_layout,
            texture_bind_groups: Vec::new(),
            instance_buffer,
            instance_buffer_capacity: 256,
            terrain_geometry: None,
            terrain_pipeline,
            terrain_buffer,
            terrain_bind_group,
            terrain_bind_group_layout,
            terrain_last_splat: None,
            terrain_last_layer0: None,
            terrain_last_layer1: None,
            terrain_last_layer2: None,
            terrain_last_layer3: None,
            color_handle: None,
            depth_handle: None,
            batches: Arc::from([]),
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            light_color: Vec3::ONE,
            ambient: Vec3::splat(0.1),
            has_water: false,
            reflection_enabled: false,
            reflection_level: 0.0,
        }
    }
}
