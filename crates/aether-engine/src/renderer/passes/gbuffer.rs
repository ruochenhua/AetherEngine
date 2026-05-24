//! G-Buffer Pass
//!
//! Renders scene geometry to multiple render targets:
//! - Position (RGBA16Float)
//! - Normal (RGBA16Float)
//! - Albedo (RGBA8Unorm)
//! - Material (RG8Unorm)
//!
//! Also writes to a depth buffer.

use crate::asset::mesh::{GpuMesh, Vertex};
use crate::ecs::World;
use crate::renderer::context::{GBuffer, RenderContext};
use crate::renderer::graph::RenderPass;
use glam::Mat4;
use std::sync::Arc;

/// Transform uniform data (model/view/proj matrices).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform {
    /// Model matrix.
    pub model: [[f32; 4]; 4],
    /// View matrix.
    pub view: [[f32; 4]; 4],
    /// Projection matrix.
    pub proj: [[f32; 4]; 4],
}

/// Material uniform data.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    /// Base color (rgba).
    pub albedo: [f32; 4],
    /// Roughness.
    pub roughness: f32,
    /// Metallic.
    pub metallic: f32,
    /// Padding.
    pub _pad: [f32; 2],
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            _pad: [0.0, 0.0],
        }
    }
}

/// A renderable entity (mesh + transform + material).
pub struct Renderable {
    /// GPU mesh.
    pub mesh: Arc<GpuMesh>,
    /// Model matrix.
    pub transform: Mat4,
    /// Material data.
    pub material: MaterialUniform,
}

/// G-Buffer Pass implementation.
pub struct GBufferPass {
    pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    transform_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    material_bind_group_layout: wgpu::BindGroupLayout,
    transform_buffer: wgpu::Buffer,
    material_buffer: wgpu::Buffer,
    transform_bind_group: wgpu::BindGroup,
    material_bind_group: wgpu::BindGroup,
}

impl GBufferPass {
    /// Create a new G-Buffer pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct TransformUniform {
    model: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> transform: TransformUniform;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = transform.model * vec4<f32>(in.position, 1.0);
    out.clip_position = transform.proj * transform.view * world_pos;
    out.world_pos = world_pos.xyz;
    // Transform normal to world space using the upper-left 3x3 of model matrix
    let normal_matrix = mat3x3<f32>(
        transform.model[0].xyz,
        transform.model[1].xyz,
        transform.model[2].xyz,
    );
    out.world_normal = normalize(normal_matrix * in.normal);
    out.uv = in.uv;
    return out;
}

struct MaterialUniform {
    albedo: vec4<f32>,
    roughness: f32,
    metallic: f32,
    _pad: vec2<f32>,
};

@group(1) @binding(0) var<uniform> material: MaterialUniform;

struct FragmentOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) albedo: vec4<f32>,
    @location(3) material: vec2<f32>,
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    // Encode normal from [-1,1] to [0,1] for storage in RGBA16Float
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);
    out.albedo = material.albedo;
    out.material = vec2<f32>(material.roughness, material.metallic);
    return out;
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GBuffer Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let transform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Transform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material Bind Group Layout"),
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
            label: Some("GBuffer Pipeline Layout"),
            bind_group_layouts: &[&transform_bind_group_layout, &material_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GBuffer Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rg8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let transform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transform Uniform Buffer"),
            size: std::mem::size_of::<TransformUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Material Uniform Buffer"),
            size: std::mem::size_of::<MaterialUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Transform Bind Group"),
            layout: &transform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &material_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            transform_bind_group_layout,
            material_bind_group_layout,
            transform_buffer,
            material_buffer,
            transform_bind_group,
            material_bind_group,
        }
    }

    /// Execute the G-Buffer pass.
    pub fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        gbuffer: &GBuffer,
        context: &RenderContext,
        renderables: &[Renderable],
        view: &Mat4,
        proj: &Mat4,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GBuffer Pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer.position,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer.normal,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer.albedo,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer.material,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &gbuffer.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.transform_bind_group, &[]);
        pass.set_bind_group(1, &self.material_bind_group, &[]);

        for renderable in renderables {
            let transform = TransformUniform {
                model: renderable.transform.to_cols_array_2d(),
                view: view.to_cols_array_2d(),
                proj: proj.to_cols_array_2d(),
            };
            context
                .queue
                .write_buffer(&self.transform_buffer, 0, bytemuck::cast_slice(&[transform]));
            context.queue.write_buffer(
                &self.material_buffer,
                0,
                bytemuck::cast_slice(&[renderable.material]),
            );

            pass.set_vertex_buffer(0, renderable.mesh.vertex_buffer.slice(..));
            if let Some(ref index_buffer) = renderable.mesh.index_buffer {
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..renderable.mesh.index_count, 0, 0..1);
            } else {
                pass.draw(0..renderable.mesh.vertex_count, 0..1);
            }
        }
    }
}

impl RenderPass for GBufferPass {
    fn name(&self) -> &str {
        "GBuffer"
    }

    fn execute(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        _context: &RenderContext,
        _world: &World,
    ) {
        // The typed execute() above is preferred; this trait method is a placeholder
        // for RenderGraph integration in future phases.
    }
}