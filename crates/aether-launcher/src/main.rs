//! Aether Engine Launcher
//!
//! Thin orchestration layer. Builds the Scheduler from passes,
//! injects per-frame data via `RenderFrame`, and delegates execution.

use aether_engine::{
    asset::registry::BuiltinMeshRegistry,
    input::InputManager,
    renderer::{
        camera::FlyCamera,
        context::RenderContext,
        frame::RenderFrame,
        ibl::{IblConfig, IblResources},
        light::LightingUniforms,
        passes::{
            debug::DebugLinePass,
            gbuffer::GBufferPass,
            lighting::LightingPass,
            shadow::ShadowPass,
        },
        scheduler::PipelineBuilder,
    },
    scene::loader::{SceneLoader, SceneResources},
};
use std::sync::Arc;
use tracing::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::WindowBuilder,
};

// ── Launcher state ──────────────────────────────────────────────────

enum LauncherState {
    Menu,
    Running {
        resources: SceneResources,
        lighting: LightingUniforms,
    },
}

struct SceneEntry {
    name: String,
    path: std::path::PathBuf,
}

fn discover_scenes() -> Vec<SceneEntry> {
    let mut entries = Vec::new();
    let scenes_dir = std::path::Path::new("scenes");
    if !scenes_dir.is_dir() {
        return entries;
    }
    let Ok(dir) = std::fs::read_dir(scenes_dir) else {
        return entries;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "ron") {
            let name = match SceneLoader::from_file(&path) {
                Ok(desc) => desc.name,
                Err(_) => path.file_stem().unwrap_or_default().to_string_lossy().into(),
            };
            entries.push(SceneEntry { name, path });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

// ── Main ────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt::init();
    info!("Aether Engine Launcher starting...");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Aether Engine Launcher")
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut ctx = pollster::block_on(RenderContext::new(&window));

    let egui_ctx = egui::Context::default();
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_winit_state =
        egui_winit::State::new(egui_ctx.clone(), viewport_id, &window, None, None);
    let mut egui_renderer =
        egui_wgpu::Renderer::new(&ctx.device, ctx.surface_format(), None, 1);

    let mut input = InputManager::new();
    let mesh_registry = BuiltinMeshRegistry::new();
    let surface_format = ctx.surface_format();
    let depth_format = wgpu::TextureFormat::Depth32Float;

    // Create placeholder IBL resources (uninitialized cubemaps for now).
    // Will be filled with real HDR data via compute shaders in the future.
    let ibl_resources = IblResources::generate(
        &ctx.device,
        None,
        &IblConfig::default(),
    );

    // Build scheduler: validates the pass graph, allocates textures, resolves passes.
    let mut scheduler = PipelineBuilder::new()
        .add(ShadowPass::new(&ctx.device))
        .add(GBufferPass::new(&ctx.device))
        .add(LightingPass::new_with_ibl(&ctx.device, surface_format, &ibl_resources))
        .add(DebugLinePass::new(&ctx.device, surface_format, depth_format))
        .build(&ctx.device, ctx.config.width, ctx.config.height);

    // Camera
    let mut camera = FlyCamera {
        position: glam::Vec3::new(3.0, 3.0, 3.0),
        yaw: -std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2,
        pitch: -std::f32::consts::FRAC_PI_4,
        speed: 4.0,
        base_speed: 4.0,
        min_speed: 0.1,
        max_speed: 100.0,
        sensitivity: 0.002,
        active: false,
        fov: 45.0f32.to_radians(),
        near: 0.1,
        far: 1000.0,
    };

    let mut last_frame_time = std::time::Instant::now();
    let mut scroll_input: f32 = 0.0;
    let mut fps: f32 = 0.0;

    let scene_entries = discover_scenes();
    info!("Discovered {} scenes", scene_entries.len());

    let mut state = LauncherState::Menu;
    let mut pending_load: Option<usize> = None;
    let mut pending_back = false;
    let mut show_overlay = false;
    let mut debug_mode: i32 = 0;

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
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(size)
                            if size.width > 0 && size.height > 0 =>
                        {
                            ctx.resize(size.width, size.height);
                            scheduler.rebuild(&ctx.device, size.width, size.height);
                        }
                        WindowEvent::RedrawRequested => {
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame_time).as_secs_f32();
                            last_frame_time = now;
                            if dt > 0.0 {
                                fps = fps * 0.9 + (1.0 / dt) * 0.1;
                            }

                            if matches!(state, LauncherState::Running { .. })
                                && input.key_pressed(KeyCode::Escape)
                            {
                                pending_back = true;
                            }

                            if input.key_pressed(KeyCode::Digit0) { debug_mode = 0; }
                            if input.key_pressed(KeyCode::Digit1) { debug_mode = 1; }
                            if input.key_pressed(KeyCode::Digit2) { debug_mode = 2; }
                            if input.key_pressed(KeyCode::Digit3) { debug_mode = 3; }
                            if input.key_pressed(KeyCode::Digit4) { debug_mode = 4; }
                            if input.key_pressed(KeyCode::Digit5) { debug_mode = 5; }
                            if input.key_pressed(KeyCode::Digit6) { debug_mode = 6; }
                            if input.key_pressed(KeyCode::Digit7) { debug_mode = 7; }
                            if input.key_pressed(KeyCode::Digit8) { debug_mode = 8; }

                            if let Some(idx) = pending_load.take() {
                                let entry = &scene_entries[idx];
                                match SceneLoader::from_file(&entry.path) {
                                    Ok(desc) => {
                                        camera.position =
                                            glam::Vec3::from_array(desc.camera.position);
                                        camera.yaw = desc.camera.yaw;
                                        camera.pitch = desc.camera.pitch;
                                        camera.speed = desc.camera.speed;
                                        camera.base_speed = desc.camera.speed;
                                        camera.fov = desc.camera.fov.to_radians();
                                        camera.active = false;

                                        match SceneLoader::build_resources(
                                            &desc, &ctx.device, &mesh_registry,
                                        ) {
                                            Ok(resources) => {
                                                let lighting =
                                                    resources.lighting_uniforms;
                                                state = LauncherState::Running {
                                                    resources,
                                                    lighting,
                                                };
                                                show_overlay = false;
                                            }
                                            Err(e) => {
                                                error!("Build scene error: {:?}", e);
                                            }
                                        }
                                    }
                                    Err(e) => error!("Load scene error: {:?}", e),
                                }
                            }

                            if pending_back {
                                state = LauncherState::Menu;
                                show_overlay = false;
                                pending_back = false;
                            }

                            if matches!(state, LauncherState::Running { .. }) {
                                let (dx, dy) = input.mouse_delta();
                                camera.update(dt, dx, dy, scroll_input, &input);
                                scroll_input = 0.0;
                            }

                            // egui UI
                            let raw_input = egui_winit_state.take_egui_input(&window);
                            let egui_output = egui_ctx.run(raw_input, |ctx| {
                                scroll_input +=
                                    ctx.input(|i| i.smooth_scroll_delta.y * 0.01);

                                match &state {
                                    LauncherState::Menu => {
                                        egui::CentralPanel::default().show(ctx, |ui| {
                                            ui.heading("Aether Engine Scenes");
                                            ui.separator();
                                            if scene_entries.is_empty() {
                                                ui.label("No scenes found.");
                                            } else {
                                                for (idx, entry) in
                                                    scene_entries.iter().enumerate()
                                                {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{}. {}",
                                                                idx + 1, entry.name
                                                            ))
                                                            .monospace()
                                                            .strong(),
                                                        );
                                                        if ui.button("▶ Launch").clicked() {
                                                            pending_load = Some(idx);
                                                        }
                                                    });
                                                    ui.separator();
                                                }
                                            }
                                        });
                                    }
                                    LauncherState::Running { resources, .. } => {
                                        egui::Area::new("menu_btn".into())
                                            .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
                                            .show(ctx, |ui| {
                                                if ui
                                                    .button(egui::RichText::new("≡").size(20.0).strong())
                                                    .clicked()
                                                {
                                                    show_overlay = !show_overlay;
                                                }
                                            });
                                        if show_overlay {
                                            egui::Window::new("Aether Launcher")
                                                .resizable(false)
                                                .collapsible(false)
                                                .anchor(
                                                    egui::Align2::LEFT_TOP,
                                                    [48.0, 8.0],
                                                )
                                                .show(ctx, |ui| {
                                                    ui.heading("Scene Info");
                                                    ui.separator();
                                                    ui.label(format!("FPS: {:.1}", fps));
                                                    ui.label(format!(
                                                        "Frame: {:.2} ms",
                                                        dt * 1000.0
                                                    ));
                                                    ui.label(format!(
                                                        "Renderables: {}",
                                                        resources.renderables.len()
                                                    ));
                                                    let p = camera.position;
                                                    ui.label(format!(
                                                        "Camera: ({:.1}, {:.1}, {:.1})",
                                                        p.x, p.y, p.z
                                                    ));
                                                    ui.label(format!(
                                                        "Speed: {:.1}",
                                                        camera.speed
                                                    ));
                                                    ui.label(format!(
                                                        "FlyCam: {}",
                                                        if camera.active {
                                                            "◉ ACTIVE"
                                                        } else {
                                                            "○ IDLE"
                                                        }
                                                    ));
                                                    let mode_names = [
                                                        "Full", "Ambient", "Diffuse",
                                                        "Specular", "Normals", "NdotL",
                                                        "Shadow", "DirectOnly", "IBLOnly",
                                                    ];
                                                    let mode_idx =
                                                        debug_mode.clamp(0, 8) as usize;
                                                    ui.label(format!(
                                                        "Debug: [{}] {}",
                                                        mode_idx, mode_names[mode_idx]
                                                    ));
                                                    ui.separator();
                                                    if ui.button("Back to Menu (Esc)").clicked()
                                                    {
                                                        pending_back = true;
                                                    }
                                                });
                                        }
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

                            // Acquire surface
                            let output = match ctx.get_current_texture() {
                                Ok(o) => o,
                                Err(wgpu::SurfaceError::Lost) => {
                                    ctx.resize(ctx.config.width, ctx.config.height);
                                    return;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    elwt.exit();
                                    return;
                                }
                                Err(e) => {
                                    error!("Surface: {:?}", e);
                                    return;
                                }
                            };
                            let target_view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = ctx.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("Encoder"),
                                },
                            );

                            match &state {
                                LauncherState::Menu => {
                                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("Menu"),
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
                                    });
                                }
                                LauncherState::Running {
                                                    ref resources,
                                                    ref lighting,
                                                } => {
                                    let aspect =
                                        ctx.config.width as f32 / ctx.config.height as f32;

                                    // Build per-frame context — all passes extract
                                    // what they need via apply_frame.
                                    let frame = RenderFrame {
                                        renderables: &resources.renderables,
                                        camera: &camera,
                                        lighting,
                                        queue: &ctx.queue,
                                        aspect,
                                        delta_time: dt,
                                    };
                                    scheduler.apply_frame_all(&frame);
                                    scheduler.execute_all(&mut encoder, &target_view);
                                }
                            }

                            // egui pass
                            for (id, image_delta) in &egui_output.textures_delta.set {
                                egui_renderer.update_texture(
                                    &ctx.device, &ctx.queue, *id, image_delta,
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
                                let mut rp = encoder.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("egui"),
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
                                    &mut rp,
                                    &paint_jobs,
                                    &screen_descriptor,
                                );
                            }

                            ctx.queue.submit(std::iter::once(encoder.finish()));
                            output.present();
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
