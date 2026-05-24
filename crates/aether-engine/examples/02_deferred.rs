//! 02_deferred - Deferred Shading with Lighting + Debug Overlay
//!
//! Renders a cube and sphere with Blinn-Phong lighting.
//! Includes egui debug overlay (FPS, frame time, resolution).

use aether_engine::asset::mesh::{CpuMesh, GpuMesh};
use aether_engine::renderer::context::{GBuffer, RenderContext};
use aether_engine::renderer::passes::gbuffer::{GBufferPass, MaterialUniform, Renderable};
use aether_engine::renderer::passes::lighting::{LightingPass, LightingUniforms};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use tracing::info;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    tracing_subscriber::fmt::init();
    info!("Aether Engine - Deferred Shading Example starting...");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Aether Engine - Deferred Shading")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut ctx = pollster::block_on(RenderContext::new(&window));

    // --- Create G-Buffer ---
    let mut gbuffer = GBuffer::new(&ctx.device, ctx.config.width, ctx.config.height);

    // --- Create render passes ---
    let gbuffer_pass = GBufferPass::new(&ctx.device);
    let mut lighting_pass = LightingPass::new(&ctx.device, &gbuffer, &ctx.config);

    // --- Create meshes ---
    let cube_cpu = CpuMesh::cube();
    let sphere_cpu = CpuMesh::sphere(16);
    let cube_gpu = GpuMesh::from_cpu(&ctx.device, &cube_cpu);
    let sphere_gpu = GpuMesh::from_cpu(&ctx.device, &sphere_cpu);

    let cube_renderable = Renderable {
        mesh: Arc::new(cube_gpu),
        transform: Mat4::from_translation(Vec3::new(-0.8, 0.0, 0.0)),
        material: MaterialUniform {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            _pad: [0.0, 0.0],
        },
    };

    let sphere_renderable = Renderable {
        mesh: Arc::new(sphere_gpu),
        transform: Mat4::from_translation(Vec3::new(0.8, 0.0, 0.0)),
        material: MaterialUniform {
            albedo: [0.2, 0.5, 0.8, 1.0],
            roughness: 0.3,
            metallic: 0.1,
            _pad: [0.0, 0.0],
        },
    };

    let renderables = vec![cube_renderable, sphere_renderable];

    // --- Camera matrices (temporary) ---
    let eye = Vec3::new(3.0, 3.0, 3.0);
    let target = Vec3::ZERO;
    let up = Vec3::Y;
    let view = Mat4::look_at_rh(eye, target, up);
    let proj = Mat4::perspective_rh(45.0f32.to_radians(), 1280.0 / 720.0, 0.1, 100.0);

    // --- Lighting uniforms ---
    let lighting_uniforms = LightingUniforms {
        camera_pos: eye.into(),
        ..Default::default()
    };
    lighting_pass.update_uniforms(&ctx.queue, &lighting_uniforms);

    // --- egui setup ---
    let egui_ctx = egui::Context::default();
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_winit_state =
        egui_winit::State::new(egui_ctx.clone(), viewport_id, &window, None, None);
    let mut egui_renderer = egui_wgpu::Renderer::new(&ctx.device, ctx.config.format, None, 1);

    // --- Frame timing ---
    let mut last_frame_time = std::time::Instant::now();
    let mut fps: f32 = 0.0;

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => {
                    let egui_response = egui_winit_state.on_window_event(&window, &event);
                    if egui_response.consumed {
                        return;
                    }

                    match event {
                        WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            elwt.exit();
                        }
                        WindowEvent::Resized(physical_size)
                            if physical_size.width > 0 && physical_size.height > 0 => {
                                ctx.resize(physical_size.width, physical_size.height);
                                gbuffer = GBuffer::new(
                                    &ctx.device,
                                    physical_size.width,
                                    physical_size.height,
                                );
                                lighting_pass.recreate_bind_group(&ctx.device, &gbuffer);
                            }
                        WindowEvent::RedrawRequested => {
                            // --- Timing ---
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame_time).as_secs_f32();
                            last_frame_time = now;
                            let frame_time_ms = dt * 1000.0;
                            if dt > 0.0 {
                                fps = fps * 0.9 + (1.0 / dt) * 0.1;
                            }

                            // --- Prepare egui ---
                            let raw_input = egui_winit_state.take_egui_input(&window);
                            let egui_output = egui_ctx.run(raw_input, |ctx| {
                                egui::Window::new("Aether Debug")
                                    .resizable(false)
                                    .collapsible(false)
                                    .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
                                    .show(ctx, |ui| {
                                        ui.label(format!("FPS: {:.1}", fps));
                                        ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                                        ui.label(format!(
                                            "Resolution: {}x{}",
                                            ctx.screen_rect().width() as u32,
                                            ctx.screen_rect().height() as u32
                                        ));
                                    });
                            });
                            egui_winit_state
                                .handle_platform_output(&window, egui_output.platform_output);

                            let paint_jobs = egui_ctx
                                .tessellate(egui_output.shapes, egui_output.pixels_per_point);
                            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [ctx.config.width, ctx.config.height],
                                pixels_per_point: window.scale_factor() as f32,
                            };

                            // --- Render ---
                            let output = match ctx.surface.get_current_texture() {
                                Ok(o) => o,
                                Err(wgpu::SurfaceError::Lost) => {
                                    ctx.resize(ctx.config.width, ctx.config.height);
                                    return;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    tracing::error!("GPU out of memory");
                                    elwt.exit();
                                    return;
                                }
                                Err(e) => {
                                    tracing::error!("Surface error: {:?}", e);
                                    return;
                                }
                            };
                            let texture_view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = ctx.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("Render Encoder"),
                                },
                            );

                            // 1. G-Buffer pass
                            gbuffer_pass.execute(
                                &mut encoder,
                                &gbuffer,
                                &ctx,
                                &renderables,
                                &view,
                                &proj,
                            );

                            // 2. Lighting pass
                            lighting_pass.execute(&mut encoder, &texture_view);

                            // 3. egui pass
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
                                let mut render_pass =
                                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("egui Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &texture_view,
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
                                    });
                                egui_renderer.render(
                                    &mut render_pass,
                                    &paint_jobs,
                                    &screen_descriptor,
                                );
                            }

                            ctx.queue.submit(std::iter::once(encoder.finish()));
                            output.present();
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
