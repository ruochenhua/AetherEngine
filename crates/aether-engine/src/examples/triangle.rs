//! 01_triangle – Minimal colored triangle.
//!
//! Demonstrates the simplest possible render pipeline: a hard-coded vertex
//! buffer, a single shader, and a clear-to-color background.

use crate::{
    examples::{Example, ExampleStateSnapshot},
    input::InputManager,
    renderer::context::RenderContext,
};
use wgpu::util::DeviceExt;

// ---------------------------------------------------------------------------
// Vertex data
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [0.0, 0.0, 1.0],
    },
];

// ---------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------

const SHADER_WGSL: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

// ---------------------------------------------------------------------------
// Example implementation
// ---------------------------------------------------------------------------

/// Minimal colored triangle example.
pub struct TriangleExample {
    render_pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    last_frame_time: std::time::Instant,
    fps: f32,
}

impl TriangleExample {
    /// Create a new triangle example (GPU resources are *not* allocated yet).
    pub fn new() -> Self {
        Self {
            render_pipeline: None,
            vertex_buffer: None,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
        }
    }
}

impl Default for TriangleExample {
    fn default() -> Self {
        Self::new()
    }
}

impl Example for TriangleExample {
    fn name(&self) -> &'static str {
        "01_triangle"
    }

    fn description(&self) -> &'static str {
        "Minimal bootstrap: winit window, wgpu context, colored triangle, egui overlay."
    }

    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()> {
        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Triangle Shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
            });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        self.render_pipeline = Some(ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: ctx.surface_format(),
                        blend: Some(wgpu::BlendState::REPLACE),
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
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
            }));

        self.vertex_buffer = Some(ctx.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        self.last_frame_time = std::time::Instant::now();
        self.fps = 0.0;

        Ok(())
    }

    fn cleanup(&mut self, _ctx: &RenderContext) {
        self.render_pipeline = None;
        self.vertex_buffer = None;
    }

    fn update(&mut self, _ctx: &RenderContext, _dt: f32, _input: &InputManager) {
        // Static triangle – no per-frame logic.
    }

    fn prepare(&mut self, _ctx: &RenderContext) {
        // No GPU uploads needed per frame.
    }

    fn render(
        &mut self,
        _ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Triangle Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.15,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(self.render_pipeline.as_ref().unwrap());
        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().slice(..));
        render_pass.draw(0..3, 0..1);

        Ok(())
    }

    fn ui(&mut self, egui_ctx: &egui::Context) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        let frame_time_ms = dt * 1000.0;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }

        egui::Window::new("Aether Debug")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
            .show(egui_ctx, |ui| {
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                ui.label(format!(
                    "Resolution: {}x{}",
                    egui_ctx.screen_rect().width() as u32,
                    egui_ctx.screen_rect().height() as u32
                ));
            });
    }

    fn resize(&mut self, _ctx: &RenderContext, _width: u32, _height: u32) {
        // Full-screen triangle – no resize logic needed.
    }

    fn snapshot(&self) -> Option<ExampleStateSnapshot> {
        Some(ExampleStateSnapshot {
            fps: self.fps,
            frame_time_ms: if self.fps > 0.0 { 1000.0 / self.fps } else { 0.0 },
            ..Default::default()
        })
    }
}
