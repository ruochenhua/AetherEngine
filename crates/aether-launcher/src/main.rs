//! Aether Engine Example Launcher
//!
//! Unified entry point for all engine demos. Creates a single window and
//! `RenderContext` shared across every example. Switching examples does not
//! recreate the window or the GPU device.

use aether_engine::{
    examples::{DeferredExample, Example, GltfSceneExample, TriangleExample},
    input::InputManager,
    renderer::context::RenderContext,
};
use std::sync::Arc;
use tracing::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::WindowBuilder,
};

// ---------------------------------------------------------------------------
// Example registry
// ---------------------------------------------------------------------------

struct RegistryEntry {
    name: &'static str,
    description: &'static str,
    factory: Box<dyn Fn() -> Box<dyn Example>>,
}

/// Declarative example registration.  Adding a new example only requires
/// appending one line here.
macro_rules! register_examples {
    ($($name:expr => $factory:expr, $desc:expr),* $(,)?) => {
        fn build_registry() -> Vec<RegistryEntry> {
            vec![
                $(RegistryEntry {
                    name: $name,
                    description: $desc,
                    factory: Box::new($factory),
                }),*
            ]
        }
    };
}

register_examples!(
    "01_triangle" => || Box::new(TriangleExample::new()),
        "Minimal bootstrap: winit window, wgpu context, colored triangle.",
    "02_deferred" => || Box::new(DeferredExample::new()),
        "Deferred shading: cube + sphere with Blinn-Phong lighting and tonemap.",
    "03_gltf_scene" => || Box::new(GltfSceneExample::with_default_model()),
        "Scene loading with GLTF/OBJ + Deferred shading + Shadow mapping.",
);

// ---------------------------------------------------------------------------
// Launcher main
// ---------------------------------------------------------------------------

fn main() {
    tracing_subscriber::fmt::init();
    info!("Aether Engine Launcher starting...");

    let registry = build_registry();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Aether Engine Launcher")
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut ctx = pollster::block_on(RenderContext::new(&window));

    // Shared infrastructure
    let egui_ctx = egui::Context::default();
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_winit_state =
        egui_winit::State::new(egui_ctx.clone(), viewport_id, &window, None, None);
    let mut egui_renderer =
        egui_wgpu::Renderer::new(&ctx.device, ctx.surface_format(), None, 1);

    let mut input = InputManager::new();
    let mut last_frame_time = std::time::Instant::now();

    // Launcher state
    let mut active_example: Option<Box<dyn Example>> = None;
    let mut show_menu = true;
    let mut pending_switch: Option<usize> = None;
    let mut pending_back = false;
    let mut show_overlay = false; // toggled by ≡ button

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
                            if let Some(ref mut ex) = active_example {
                                ex.cleanup(&ctx);
                            }
                            elwt.exit();
                        }
                        WindowEvent::Resized(physical_size)
                            if physical_size.width > 0 && physical_size.height > 0 =>
                        {
                            ctx.resize(physical_size.width, physical_size.height);
                            if let Some(ref mut ex) = active_example {
                                ex.resize(&ctx, physical_size.width, physical_size.height);
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            // --------------------------------------------------
                            // Timing
                            // --------------------------------------------------
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame_time).as_secs_f32();
                            last_frame_time = now;
                            let fps = 1.0 / dt.max(0.0001);

                            // --------------------------------------------------
                            // Input: Esc returns to menu
                            // --------------------------------------------------
                            if !show_menu && input.key_pressed(KeyCode::Escape) {
                                pending_back = true;
                            }

                            // --------------------------------------------------
                            // Apply pending state transitions
                            // --------------------------------------------------
                            if let Some(idx) = pending_switch.take() {
                                if let Some(ref mut ex) = active_example {
                                    ex.cleanup(&ctx);
                                }
                                let mut new_ex = (registry[idx].factory)();
                                if let Err(e) = new_ex.init(&ctx) {
                                    error!("Example init failed: {:?}", e);
                                } else {
                                    active_example = Some(new_ex);
                                    show_menu = false;
                                    show_overlay = false;
                                    info!("Switched to example: {}", registry[idx].name);
                                }
                            }
                            if pending_back {
                                if let Some(ref mut ex) = active_example {
                                    ex.cleanup(&ctx);
                                }
                                active_example = None;
                                show_menu = true;
                                show_overlay = false;
                                pending_back = false;
                                info!("Returned to menu");
                            }

                            // --------------------------------------------------
                            // Update + Prepare (only when running an example)
                            // --------------------------------------------------
                            if let Some(ref mut ex) = active_example {
                                ex.update(&ctx, dt, &input);
                                ex.prepare(&ctx);
                            }

                            // --------------------------------------------------
                            // egui UI
                            // --------------------------------------------------
                            let raw_input = egui_winit_state.take_egui_input(&window);
                            let egui_output = egui_ctx.run(raw_input, |ctx| {
                                if show_menu {
                                    // --------------------------------------------------
                                    // Menu mode
                                    // --------------------------------------------------
                                    egui::CentralPanel::default().show(ctx, |ui| {
                                        ui.heading("Aether Engine Examples");
                                        ui.separator();
                                        for (idx, entry) in registry.iter().enumerate() {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{}. {}",
                                                        idx + 1,
                                                        entry.name
                                                    ))
                                                    .monospace()
                                                    .strong(),
                                                );
                                                if ui.button("▶ Launch").clicked() {
                                                    pending_switch = Some(idx);
                                                }
                                            });
                                            ui.label(entry.description);
                                            ui.separator();
                                        }
                                    });
                                } else {
                                    // --------------------------------------------------
                                    // Runtime:常驻 ≡ 按钮
                                    // --------------------------------------------------
                                    egui::Area::new("launcher_menu_button".into())
                                        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
                                        .show(ctx, |ui| {
                                            if ui
                                                .button(
                                                    egui::RichText::new("≡")
                                                        .size(20.0)
                                                        .strong(),
                                                )
                                                .clicked()
                                            {
                                                show_overlay = !show_overlay;
                                            }
                                        });

                                    // --------------------------------------------------
                                    // Overlay panel (toggled by ≡)
                                    // --------------------------------------------------
                                    if show_overlay {
                                        egui::Window::new("Aether Launcher")
                                            .resizable(false)
                                            .collapsible(false)
                                            .anchor(egui::Align2::LEFT_TOP, [48.0, 8.0])
                                            .show(ctx, |ui| {
                                                if let Some(ref ex) = active_example {
                                                    ui.heading(ex.name());
                                                    ui.separator();

                                                    // Global timing
                                                    ui.label(format!("FPS: {:.1}", fps));
                                                    ui.label(format!(
                                                        "Frame: {:.2} ms",
                                                        dt * 1000.0
                                                    ));

                                                    // Snapshot from example
                                                    if let Some(snap) = ex.snapshot() {
                                                        ui.separator();
                                                        ui.label(
                                                            egui::RichText::new("Snapshot")
                                                                .small()
                                                                .weak(),
                                                        );
                                                        if let Some(n) = snap.renderable_count {
                                                            ui.label(format!("Renderables: {}", n));
                                                        }
                                                        if let Some(pos) = snap.camera_position {
                                                            ui.label(format!(
                                                                "Camera: {:.2}, {:.2}, {:.2}",
                                                                pos[0], pos[1], pos[2]
                                                            ));
                                                        }
                                                        if let Some(n) = snap.entity_count {
                                                            ui.label(format!("Entities: {}", n));
                                                        }
                                                        for (k, v) in &snap.custom {
                                                            ui.label(format!("{}: {}", k, v));
                                                        }
                                                    }

                                                    ui.separator();
                                                    if ui.button("Back to Menu (Esc)").clicked() {
                                                        pending_back = true;
                                                    }
                                                }
                                            });
                                    }

                                    // Example-specific UI
                                    if let Some(ref mut ex) = active_example {
                                        ex.ui(ctx);
                                    }
                                }
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
                                    label: Some("LauncherRenderEncoder"),
                                },
                            );

                            if show_menu {
                                // Menu background
                                encoder.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("Menu Background"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &target_view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                                        r: 0.08,
                                                        g: 0.08,
                                                        b: 0.1,
                                                        a: 1.0,
                                                    }),
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    },
                                );
                            } else if let Some(ref mut ex) = active_example {
                                if let Err(e) = ex.render(&ctx, &mut encoder, &target_view) {
                                    error!("Example render error: {:?}", e);
                                }
                            }

                            // --------------------------------------------------
                            // egui pass (drawn on top)
                            // --------------------------------------------------
                            for (id, image_delta) in &egui_output.textures_delta.set {
                                egui_renderer.update_texture(
                                    &ctx.device,
                                    &ctx.queue,
                                    *id,
                                    image_delta,
                                );
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
