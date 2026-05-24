//! Rendering module.
//!
//! Core rendering system built on wgpu. Features:
//! - RenderGraph-based pass scheduling
//! - Deferred shading pipeline
//! - PBR material support
//! - Extensible pass system

pub mod camera;
pub mod context;
pub mod graph;
pub mod light;
pub mod mesh;
pub mod passes;

use crate::ecs::World;
use context::RenderContext;
use graph::RenderGraph;
use tracing::{info, trace};

/// Main renderer.
pub struct Renderer {
    graph: RenderGraph,
    width: u32,
    height: u32,
    frame_count: u64,
}

impl Renderer {
    /// Create a new renderer.
    pub fn new(_context: &mut RenderContext, width: u32, height: u32) -> Self {
        info!("Initializing renderer ({}x{})", width, height);

        let graph = RenderGraph::new();

        Self {
            graph,
            width,
            height,
            frame_count: 0,
        }
    }

    /// Update renderer state.
    pub fn update(&mut self, _dt: f32) {
        // TODO: Update camera, animations, etc.
    }

    /// Render a frame.
    pub fn render(&mut self, context: &mut RenderContext) -> Result<(), wgpu::SurfaceError> {
        let _frame_start = std::time::Instant::now();

        let output = context.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = context.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Execute render graph
        // self.graph.execute(&mut encoder, &view, context, &world);

        // Temporary: clear to black
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        context.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.frame_count += 1;
        trace!("Frame {} rendered", self.frame_count);

        Ok(())
    }

    /// Resize the renderer.
    pub fn resize(&mut self, context: &mut RenderContext, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        info!("Resizing renderer to {}x{}", width, height);
        context.resize(width, height);
        self.width = width;
        self.height = height;

        // TODO: Resize render graph resources
    }

    /// Get the current resolution.
    pub fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
