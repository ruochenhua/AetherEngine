//! Water pass pipeline and mesh creation.
//!
//! Creates the render pipeline, bind group layouts, uniform buffer,
//! and the subdivided water plane mesh used by [`WaterPass`].

use super::WaterPass;
use super::WaterUniform;
use crate::asset::mesh::{CpuMesh, GpuMesh, Vertex};
use crate::asset::texture::{CpuTexture, GpuTexture};
use std::borrow::Cow;
use std::mem::size_of;
use std::sync::Arc;

mod shaders;

impl WaterPass {
    /// Create a new water pass.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = shaders::WATER_SHADER_SRC;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Water Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Water Uniform Bind Group Layout"),
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

        let texture_bind_group_layout = create_texture_bind_group_layout(device);
        let water_texture_bind_group_layout = create_water_texture_bind_group_layout(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Water Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
                Some(&water_texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

        let usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        #[cfg(test)]
        let usage = usage | wgpu::BufferUsages::COPY_SRC;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Water Uniform Buffer"),
            size: size_of::<WaterUniform>() as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Neutral fallback textures: grey dudv (no distortion) and flat normal.
        let fallback_dudv = Arc::new(GpuTexture::from_cpu(
            device,
            queue,
            &CpuTexture::from_color(128, 128, 0, 255),
            Some("water_fallback_dudv"),
        ));
        let fallback_normal = Arc::new(GpuTexture::from_cpu(
            device,
            queue,
            &CpuTexture::from_color(128, 128, 255, 255),
            Some("water_fallback_normal"),
        ));
        let water_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Water Texture Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let scene_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Water Scene Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let water_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Material Bind Group"),
            layout: &water_texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fallback_dudv.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&water_sampler),
                },
            ],
        });

        let mesh = Arc::new(GpuMesh::from_cpu(device, &create_water_plane(128, 1024.0)));

        Self {
            device: device.clone(),
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group: None,
            water_texture_bind_group: Some(water_texture_bind_group),
            water_texture_bind_group_layout,
            water_sampler,
            scene_sampler,
            fallback_dudv: fallback_dudv.clone(),
            fallback_normal: fallback_normal.clone(),
            mesh,
            scene_color_handle: None,
            reflection_handle: None,
            planar_reflection_handle: None,
            depth_handle: None,
            water_color_handle: None,
            has_water: false,
            time: 0.0,
            last_dudv: Some(fallback_dudv),
            last_normal: Some(fallback_normal),
        }
    }
}

/// Create the texture bind group layout shared by the water pipeline and
/// the per-frame texture bind group (scene color + reflection + sampler).
pub(super) fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Water Texture Bind Group Layout"),
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
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
        ],
    })
}

/// Create the bind group layout for water material textures (dudv + normal).
pub(super) fn create_water_texture_bind_group_layout(
    device: &wgpu::Device,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Water Material Bind Group Layout"),
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
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Create a subdivided XZ-plane CPU mesh centered at the origin.
fn create_water_plane(subdivisions: u32, extent: f32) -> CpuMesh {
    let segments = subdivisions.max(1);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for z in 0..=segments {
        for x in 0..=segments {
            let u = x as f32 / segments as f32;
            let v = z as f32 / segments as f32;
            let px = (u - 0.5) * extent;
            let pz = (v - 0.5) * extent;
            positions.push([px, 0.0, pz]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([u * 4.0, v * 4.0]);
        }
    }

    for z in 0..segments {
        for x in 0..segments {
            let i0 = z * (segments + 1) + x;
            let i1 = i0 + 1;
            let i2 = (z + 1) * (segments + 1) + x;
            let i3 = i2 + 1;
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    CpuMesh {
        positions,
        normals,
        uvs,
        tangents: Vec::new(),
        indices,
        submeshes: Vec::new(),
    }
}
