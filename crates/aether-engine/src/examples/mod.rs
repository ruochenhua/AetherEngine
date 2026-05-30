//! Example trait and standalone runner.
//!
//! All engine examples implement the [`Example`] trait, allowing them to be
//! launched either individually via `cargo run --example` (through
//! [`run_standalone`]) or managed by the unified Launcher.

pub mod deferred;
pub mod gltf_scene;
pub mod triangle;

pub use deferred::DeferredExample;
pub use gltf_scene::GltfSceneExample;
pub use triangle::TriangleExample;

use crate::{
    input::InputManager,
    renderer::context::RenderContext,
};
use std::sync::Arc;
use tracing::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

/// Runtime state snapshot for AI introspection and Launcher overlay.
///
/// Examples populate the fields they care about; the Launcher augments
/// generic timing data before displaying or forwarding to an AI agent.
#[derive(Debug, Clone, Default)]
pub struct ExampleStateSnapshot {
    /// Current FPS (frames per second).
    pub fps: f32,
    /// Frame time in milliseconds.
    pub frame_time_ms: f32,
    /// Screen resolution (width, height).
    pub resolution: (u32, u32),
    /// Number of renderable objects (meshes) in the scene, if applicable.
    pub renderable_count: Option<usize>,
    /// Number of ECS entities, if applicable.
    pub entity_count: Option<usize>,
    /// Current camera position (x, y, z), if applicable.
    pub camera_position: Option<[f32; 3]>,
    /// Custom key-value pairs for example-specific diagnostics.
    pub custom: Vec<(String, String)>,
}

/// Public interface for all engine examples.
///
/// The Launcher and [`run_standalone`] manage an example's lifetime through
/// this trait. Implementors receive a shared [`RenderContext`] and are
/// responsible for their own GPU resources.
///
/// # Frame lifecycle (per [`run_standalone`])
///
/// 1. `update`   – CPU-side logic (camera, animation, input response)
/// 2. `prepare`  – GPU resource uploads (buffer writes, bind-group rebuilds)
/// 3. `ui`       – egui widgets rendered inside the Launcher-managed context
/// 4. `render`   – pure `CommandEncoder` recording into `target_view`
/// 5. (Launcher appends its own egui pass on top)
pub trait Example: Send {
    /// Human-readable name shown in the Launcher menu.
    fn name(&self) -> &'static str;
    /// Short description shown in the Launcher menu.
    fn description(&self) -> &'static str;

    /// Called once after the GPU context is ready.
    ///
    /// Upload meshes, create pipelines, allocate textures here.
    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()>;

    /// Called before the example is dropped or switched away.
    ///
    /// Release GPU resources to avoid memory leaks when the Launcher reuses
    /// the same `RenderContext`.
    fn cleanup(&mut self, ctx: &RenderContext);

    /// Pure CPU update, called once per frame.
    fn update(&mut self, ctx: &RenderContext, dt: f32, input: &InputManager);

    /// GPU resource upload stage, called once per frame after `update`.
    ///
    /// Use `ctx.queue.write_buffer`, rebuild bind groups, update uniform
    /// buffers, etc. Do **not** touch `CommandEncoder` here.
    fn prepare(&mut self, ctx: &RenderContext);

    /// Pure GPU command recording stage.
    ///
    /// `encoder` is fresh for this frame. `target_view` is the swap-chain
    /// texture view; the example must render its final output there.
    fn render(
        &mut self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) -> anyhow::Result<()>;

    /// Draw example-specific egui widgets.
    ///
    /// The caller (Launcher or [`run_standalone`]) already wraps this in
    /// `egui::Context::run`. Implementors only need to create `Window`s or
    /// `Panel`s inside the closure.
    fn ui(&mut self, egui_ctx: &egui::Context);

    /// Surface resolution has changed.
    ///
    /// The `RenderContext` is already reconfigured; the example should
    /// recreate any size-dependent resources (frame-buffers, projection
    /// matrices, etc.).
    fn resize(&mut self, ctx: &RenderContext, width: u32, height: u32);

    /// Return a runtime state snapshot for AI introspection.
    ///
    /// The Launcher calls this every frame and may display the data in an
    /// overlay or forward it to an external AI agent.  Default
    /// implementation returns `None` (no introspection).
    fn snapshot(&self) -> Option<ExampleStateSnapshot> {
        None
    }
}

/// Run a single example as a standalone binary.
///
/// Creates its own window, event loop, `RenderContext`, egui stack and
/// `InputManager`. This is the entry point used by `cargo run --example`.
///
/// # Example
/// ```rust,no_run
/// use aether_engine::examples::{Example, run_standalone};
///
/// struct MyExample;
/// impl Example for MyExample { /* ... */ }
///
/// fn main() {
///     run_standalone(MyExample, "my_example");
/// }
/// ```
pub fn run_standalone(mut example: impl Example, title: &str) {
    info!("Starting standalone example: {}", example.name());

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut ctx = pollster::block_on(RenderContext::new(&window));

    if let Err(e) = example.init(&ctx) {
        panic!("Example init failed: {:?}", e);
    }

    // ------------------------------------------------------------------
    // egui setup
    // ------------------------------------------------------------------
    let egui_ctx = egui::Context::default();
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_winit_state =
        egui_winit::State::new(egui_ctx.clone(), viewport_id, &window, None, None);
    let mut egui_renderer = egui_wgpu::Renderer::new(&ctx.device, ctx.surface_format(), None, 1);

    let mut input = InputManager::new();
    let mut last_frame_time = std::time::Instant::now();

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => {
                    let egui_response = egui_winit_state.on_window_event(&window, &event);
                    if !egui_response.consumed {
                        input.handle_window_event(&event);
                    }

                    match event {
                        WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            example.cleanup(&ctx);
                            elwt.exit();
                        }
                        WindowEvent::Resized(physical_size)
                            if physical_size.width > 0 && physical_size.height > 0 =>
                        {
                            ctx.resize(physical_size.width, physical_size.height);
                            example.resize(&ctx, physical_size.width, physical_size.height);
                        }
                        WindowEvent::RedrawRequested => {
                            // --------------------------------------------------
                            // Timing
                            // --------------------------------------------------
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame_time).as_secs_f32();
                            last_frame_time = now;

                            // --------------------------------------------------
                            // Update
                            // --------------------------------------------------
                            example.update(&ctx, dt, &input);

                            // --------------------------------------------------
                            // Prepare
                            // --------------------------------------------------
                            example.prepare(&ctx);

                            // --------------------------------------------------
                            // egui UI
                            // --------------------------------------------------
                            let raw_input = egui_winit_state.take_egui_input(&window);
                            let egui_output = egui_ctx.run(raw_input, |ctx| {
                                example.ui(ctx);
                            });
                            egui_winit_state
                                .handle_platform_output(&window, egui_output.platform_output);

                            let paint_jobs = egui_ctx
                                .tessellate(egui_output.shapes, egui_output.pixels_per_point);
                            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [ctx.config.width, ctx.config.height],
                                pixels_per_point: window.scale_factor() as f32,
                            };

                            // --------------------------------------------------
                            // Acquire surface
                            // --------------------------------------------------
                            let output = match ctx.get_current_texture() {
                                Ok(o) => o,
                                Err(wgpu::SurfaceError::Lost) => {
                                    ctx.resize(ctx.config.width, ctx.config.height);
                                    return;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    error!("GPU out of memory");
                                    elwt.exit();
                                    return;
                                }
                                Err(e) => {
                                    error!("Surface error: {:?}", e);
                                    return;
                                }
                            };
                            let target_view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            // --------------------------------------------------
                            // Render
                            // --------------------------------------------------
                            let mut encoder = ctx.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("StandaloneRenderEncoder"),
                                },
                            );

                            if let Err(e) = example.render(&ctx, &mut encoder, &target_view) {
                                error!("Example render error: {:?}", e);
                            }

                            // --------------------------------------------------
                            // egui pass (drawn on top)
                            // --------------------------------------------------
                            for (id, image_delta) in &egui_output.textures_delta.set {
                                egui_renderer
                                    .update_texture(&ctx.device, &ctx.queue, *id, image_delta);
                            }
                            egui_renderer.update_buffers(
                                &ctx.device,
                                &ctx.queue,
                                &mut encoder,
                                &paint_jobs,
                                &screen_descriptor,
                            );
                            {
                                let mut render_pass = encoder.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("egui Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &target_view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    },
                                );
                                egui_renderer.render(
                                    &mut render_pass,
                                    &paint_jobs,
                                    &screen_descriptor,
                                );
                            }

                            ctx.queue.submit(std::iter::once(encoder.finish()));
                            output.present();

                            // --------------------------------------------------
                            // Clear per-frame input
                            // --------------------------------------------------
                            input.end_frame();
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .expect("Event loop error");
}
