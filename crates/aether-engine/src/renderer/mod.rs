//! Rendering module.
//!
//! Core rendering system built on wgpu. Features:
//! - RenderGraph-based pass scheduling
//! - Deferred shading pipeline
//! - PBR material support
//! - Extensible pass system

/// Camera types and controllers.
pub mod camera;
/// wgpu context and device management.
pub mod context;
/// RenderGraph pass scheduling.
pub mod graph;
/// Light component definitions.
pub mod light;
/// Mesh rendering utilities.
pub mod mesh;
/// Render pass implementations.
pub mod passes;

use context::{GBuffer, RenderContext};
use glam::Mat4;
use graph::RenderGraph;
use passes::gbuffer::{GBufferPass, Renderable};
use tracing::{info, trace};

/// Main renderer.
pub struct Renderer {
    _graph: RenderGraph,
    gbuffer_pass: GBufferPass,
    gbuffer: Option<GBuffer>,
    width: u32,
    height: u32,
    frame_count: u64,
}

impl Renderer {
    /// Create a new renderer.
    pub fn new(context: &RenderContext, width: u32, height: u32) -> Self {
        info!("Initializing renderer ({}x{})", width, height);

        let graph = RenderGraph::new();
        let gbuffer_pass = GBufferPass::new(&context.device);
        let gbuffer = Some(GBuffer::new(&context.device, width, height));

        Self {
            _graph: graph,
            gbuffer_pass,
            gbuffer,
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
    pub fn render(
        &mut self,
        context: &RenderContext,
        renderables: &[Renderable],
        view: &Mat4,
        proj: &Mat4,
    ) -> Result<(), wgpu::SurfaceError> {
        let _frame_start = std::time::Instant::now();

        let output = context.surface.get_current_texture()?;
        let output_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = context.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            },
        );

        // Execute G-Buffer pass
        if let Some(ref gbuffer) = self.gbuffer {
            self.gbuffer_pass.execute(
                &mut encoder,
                gbuffer,
                context,
                renderables,
                view,
                proj,
            );
        }

        // TODO: Lighting pass, post-process, blit to swapchain
        // For now, just copy one of the G-Buffer channels to the swapchain for visualization
        // This is temporary until we have a proper lighting/composite pass
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
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

        // Recreate G-Buffer textures
        self.gbuffer = Some(GBuffer::new(&context.device, width, height));
    }

    /// Get the current resolution.
    pub fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get a reference to the G-Buffer textures (for debugging/visualization).
    pub fn gbuffer(&self) -> Option<&GBuffer> {
        self.gbuffer.as_ref()
    }
}
