//! Debug Line Pass
//!
//! Renders colored lines for debug visualization (grid, gizmo, etc.).
//! Uses line-list topology with LoadOp::Load on color and depth —
//! draws over the existing swapchain content.
//!
//! Implements the `Pass` trait for type-safe scheduling.

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// Per-vertex data for debug lines.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DebugVertex {
    /// World-space position.
    pub position: [f32; 3],
    /// RGBA color.
    pub color: [f32; 4],
}

impl DebugVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<DebugVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// View-projection uniform for debug pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DebugUniform {
    /// View-projection matrix (view * proj).
    pub view_proj: [[f32; 4]; 4],
}

/// Debug line rendering pass.
pub struct DebugLinePass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    /// Grid vertex buffer.
    grid_vertex_buffer: wgpu::Buffer,
    grid_vertex_count: u32,
    /// Gizmo vertex buffer.
    gizmo_vertex_buffer: wgpu::Buffer,
    gizmo_vertex_count: u32,

    /// Resource handles (populated by resolve).
    depth_handle: Option<ResHandle<GDepth>>,
}

impl Pass for DebugLinePass {
    fn name(&self) -> &str {
        "DebugLine"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("DebugLine")
            .read::<GDepth>("gbuffer_depth")
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new_inner(device, wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Depth32Float)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.depth_handle = Some(resources.handle::<GDepth>("gbuffer_depth"));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let view = frame.camera.view_matrix();
        let proj = frame.camera.projection_matrix(frame.aspect);
        let vp = proj * view;
        let uniform = DebugUniform {
            view_proj: vp.to_cols_array_2d(),
        };
        frame
            .queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        surface_view: &wgpu::TextureView,
    ) {
        let depth_view = resources.get(self.depth_handle.unwrap());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Debug Line Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        // Draw grid
        pass.set_vertex_buffer(0, self.grid_vertex_buffer.slice(..));
        pass.draw(0..self.grid_vertex_count, 0..1);

        // Draw gizmo
        pass.set_vertex_buffer(0, self.gizmo_vertex_buffer.slice(..));
        pass.draw(0..self.gizmo_vertex_count, 0..1);
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl DebugLinePass {
    /// Create a new debug line pass.
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_inner(device, output_format, depth_format)
    }

    fn new_inner(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct DebugUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: DebugUniform;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Debug Line Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Debug Uniform Bind Group Layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Debug Line Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug Line Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[DebugVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Debug Uniform Buffer"),
            size: std::mem::size_of::<DebugUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Debug Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Build debug geometry vertex buffers
        let (grid_verts, grid_count) = build_grid_lines(5.0, 1.0);
        let grid_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Debug Grid VB"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let (gizmo_verts, gizmo_count) = build_gizmo_lines(0.15);
        let gizmo_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Debug Gizmo VB"),
            contents: bytemuck::cast_slice(&gizmo_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            grid_vertex_buffer,
            grid_vertex_count: grid_count,
            gizmo_vertex_buffer,
            gizmo_vertex_count: gizmo_count,
            depth_handle: None,
        }
    }

}

/// Build a debug grid on the XZ plane.
pub fn build_grid_lines(size: f32, spacing: f32) -> (Vec<DebugVertex>, u32) {
    let color = [0.3, 0.3, 0.3, 0.6];
    let mut vertices = Vec::new();

    let mut i = -size;
    while i <= size {
        vertices.push(DebugVertex { position: [i, 0.0, -size], color });
        vertices.push(DebugVertex { position: [i, 0.0, size], color });
        vertices.push(DebugVertex { position: [-size, 0.0, i], color });
        vertices.push(DebugVertex { position: [size, 0.0, i], color });
        i += spacing;
    }

    let count = vertices.len() as u32;
    (vertices, count)
}

/// Build an RGB axis gizmo at the origin.
pub fn build_gizmo_lines(length: f32) -> (Vec<DebugVertex>, u32) {
    let arrow_size = length * 0.3;
    let mut vertices = Vec::new();

    // X axis (red)
    vertices.push(DebugVertex { position: [0.0, 0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [length, 0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [length, 0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [length - arrow_size, arrow_size * 0.5, 0.0], color: [1.0, 0.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [length, 0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [length - arrow_size, -arrow_size * 0.5, 0.0], color: [1.0, 0.0, 0.0, 1.0] });

    // Y axis (green)
    vertices.push(DebugVertex { position: [0.0, 0.0, 0.0], color: [0.0, 1.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, length, 0.0], color: [0.0, 1.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, length, 0.0], color: [0.0, 1.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [arrow_size * 0.5, length - arrow_size, 0.0], color: [0.0, 1.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, length, 0.0], color: [0.0, 1.0, 0.0, 1.0] });
    vertices.push(DebugVertex { position: [-arrow_size * 0.5, length - arrow_size, 0.0], color: [0.0, 1.0, 0.0, 1.0] });

    // Z axis (blue)
    vertices.push(DebugVertex { position: [0.0, 0.0, 0.0], color: [0.0, 0.0, 1.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, 0.0, length], color: [0.0, 0.0, 1.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, 0.0, length], color: [0.0, 0.0, 1.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, arrow_size * 0.5, length - arrow_size], color: [0.0, 0.0, 1.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, 0.0, length], color: [0.0, 0.0, 1.0, 1.0] });
    vertices.push(DebugVertex { position: [0.0, -arrow_size * 0.5, length - arrow_size], color: [0.0, 0.0, 1.0, 1.0] });

    let count = vertices.len() as u32;
    (vertices, count)
}
