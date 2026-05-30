//! Debug Line Pass
//!
//! Renders colored lines for debug visualization (grid, gizmo, etc.).
//! Uses line-list topology with per-vertex colors and depth testing.

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
}

impl DebugLinePass {
    /// Create a new debug line pass.
    pub fn new(
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

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
        }
    }

    /// Update view-projection uniform.
    pub fn update_uniform(&self, queue: &wgpu::Queue, view_proj: &glam::Mat4) {
        let uniform = DebugUniform {
            view_proj: view_proj.to_cols_array_2d(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Draw debug lines with the given vertex buffer.
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        vertex_buffer: &'a wgpu::Buffer,
        vertex_count: u32,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertex_count, 0..1);
    }
}

/// Build a debug grid on the XZ plane.
///
/// `size` is half-extent (e.g. 5 for a 10×10 grid).
/// `spacing` is the distance between grid lines.
pub fn build_grid_lines(size: f32, spacing: f32) -> (Vec<DebugVertex>, u32) {
    let color = [0.3, 0.3, 0.3, 0.6];
    let mut vertices = Vec::new();

    let mut i = -size;
    while i <= size {
        // Line along Z (constant X)
        vertices.push(DebugVertex {
            position: [i, 0.0, -size],
            color,
        });
        vertices.push(DebugVertex {
            position: [i, 0.0, size],
            color,
        });
        // Line along X (constant Z)
        vertices.push(DebugVertex {
            position: [-size, 0.0, i],
            color,
        });
        vertices.push(DebugVertex {
            position: [size, 0.0, i],
            color,
        });
        i += spacing;
    }

    let count = vertices.len() as u32;
    (vertices, count)
}

/// Build an RGB axis gizmo at the origin.
///
/// `length` is the length of each axis line.
pub fn build_gizmo_lines(length: f32) -> (Vec<DebugVertex>, u32) {
    let arrow_size = length * 0.3;
    let mut vertices = Vec::new();

    // X axis (red)
    vertices.push(DebugVertex {
        position: [0.0, 0.0, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [length, 0.0, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });
    // X arrow head
    vertices.push(DebugVertex {
        position: [length, 0.0, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [length - arrow_size, arrow_size * 0.5, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [length, 0.0, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [length - arrow_size, -arrow_size * 0.5, 0.0],
        color: [1.0, 0.0, 0.0, 1.0],
    });

    // Y axis (green)
    vertices.push(DebugVertex {
        position: [0.0, 0.0, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, length, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });
    // Y arrow head
    vertices.push(DebugVertex {
        position: [0.0, length, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [arrow_size * 0.5, length - arrow_size, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, length, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [-arrow_size * 0.5, length - arrow_size, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
    });

    // Z axis (blue)
    vertices.push(DebugVertex {
        position: [0.0, 0.0, 0.0],
        color: [0.0, 0.0, 1.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, 0.0, length],
        color: [0.0, 0.0, 1.0, 1.0],
    });
    // Z arrow head
    vertices.push(DebugVertex {
        position: [0.0, 0.0, length],
        color: [0.0, 0.0, 1.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, arrow_size * 0.5, length - arrow_size],
        color: [0.0, 0.0, 1.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, 0.0, length],
        color: [0.0, 0.0, 1.0, 1.0],
    });
    vertices.push(DebugVertex {
        position: [0.0, -arrow_size * 0.5, length - arrow_size],
        color: [0.0, 0.0, 1.0, 1.0],
    });

    let count = vertices.len() as u32;
    (vertices, count)
}
