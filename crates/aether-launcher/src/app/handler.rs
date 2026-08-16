// ApplicationHandler implementation for the launcher.

use super::{App, LauncherState};
use crate::app::{input, render, scene, ui};
use aether_engine::{
    asset::texture_cache::GpuTextureCache, ecs::World, renderer::context::RenderContext,
    scene::loader::SceneLoader,
};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;
use winit::window::{WindowAttributes, WindowId};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let inner_size: winit::dpi::Size = match (self.cli.width, self.cli.height) {
            (Some(width), Some(height)) => winit::dpi::PhysicalSize::new(width, height).into(),
            (Some(width), None) => winit::dpi::PhysicalSize::new(width, 720).into(),
            (None, Some(height)) => winit::dpi::PhysicalSize::new(1280, height).into(),
            (None, None) => winit::dpi::LogicalSize::new(1280u32, 720u32).into(),
        };
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Aether Engine Launcher")
                        .with_inner_size(inner_size),
                )
                .expect("Failed to create window"),
        );

        let ctx = pollster::block_on(RenderContext::new(window.clone()));
        self.gpu_info = format!("{} ({:?})", ctx.adapter_info.name, ctx.adapter_info.backend);

        let viewport_id = self.egui_ctx.viewport_id();
        let egui_winit_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            viewport_id,
            &window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &ctx.device,
            ctx.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );

        let output_format = ctx.render_target_format();
        let depth_format = wgpu::TextureFormat::Depth32Float;

        // Initialize the GPU texture cache used by all passes.
        self.texture_cache = Some(GpuTextureCache::new(&ctx.device, &ctx.queue));

        // Build render pipeline via helper. Terrain presence is determined by
        // the scene; the empty default scene has no terrain.
        let has_terrain = false;
        let (scheduler, ibl_resources) = crate::pipeline::build_pipeline(
            &ctx.device,
            &ctx.queue,
            self.texture_cache.as_ref().unwrap(),
            output_format,
            depth_format,
            ctx.config.width,
            ctx.config.height,
            has_terrain,
        )
        .expect("Failed to build render pipeline");
        self.has_terrain_pipeline = has_terrain;
        self.ibl_resources = Some(ibl_resources);

        // Initialize GPU timer for the performance panel.
        self.gpu_timer =
            aether_engine::renderer::gpu_timer::GpuTimer::new(&ctx.device, &scheduler.pass_names());
        self.gpu_timer_supported = self.gpu_timer.as_ref().is_some_and(|t| t.supported);

        // Start with an empty scene (default camera + lighting + a default cube)
        let mut world = World::new();
        let lighting = SceneLoader::new_empty(&mut world);
        crate::pipeline::spawn_default_cube(&ctx.device, &self.mesh_registry, &mut world);
        self.state = LauncherState::Running { world, lighting };
        self.show_overlay = true;

        // Auto-open scene if --scene is provided
        scene::open_cli_scene(self, &ctx);

        self.window = Some(window);
        self.ctx = Some(ctx);
        self.egui_winit_state = Some(egui_winit_state);
        self.egui_renderer = Some(egui_renderer);
        self.scheduler = Some(scheduler);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Handle undo/redo before any ctx borrow to avoid lifetime conflicts.
        if let WindowEvent::RedrawRequested = event {
            if self.input.ctrl_held() && self.input.key_pressed(KeyCode::KeyZ) {
                self.apply_undo();
            }
            if self.input.ctrl_held() && self.input.key_pressed(KeyCode::KeyY) {
                self.apply_redo();
            }
        }

        let egui_response = self
            .egui_winit_state
            .as_mut()
            .unwrap()
            .on_window_event(self.window.as_ref().unwrap(), &event);
        // Track whether the pointer is currently over an egui UI area.
        // For any pointer event (move/click/wheel) we update the flag so
        // it always reflects the current pointer state — this prevents
        // stale "consumed" values from a previous frame blocking picking.
        match &event {
            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                self.egui_consumed_pointer = egui_response.consumed;
            }
            _ => {}
        }
        // Always track raw input state (mouse, keyboard) for the 3D viewport.
        // We guard against egui interactions at the usage site (picking / camera)
        // by checking egui_consumed_pointer during RedrawRequested.
        self.input.handle_window_event(&event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                let ctx = self.ctx.as_mut().unwrap();
                let scheduler = self.scheduler.as_mut().unwrap();
                ctx.resize(size.width, size.height);
                scheduler.rebuild(&ctx.device, size.width, size.height);
                self.gpu_timer = aether_engine::renderer::gpu_timer::GpuTimer::new(
                    &ctx.device,
                    &scheduler.pass_names(),
                );
                self.gpu_timer_supported = self.gpu_timer.as_ref().is_some_and(|t| t.supported);
            }
            WindowEvent::RedrawRequested => {
                // Reset per-frame egui pointer consumption flag.
                let egui_consumed = self.egui_consumed_pointer;
                self.egui_consumed_pointer = false;
                // Discard scroll accumulated during egui interaction
                // to avoid leaking into camera speed adjustment.
                if egui_consumed {
                    self.scroll_input = 0.0;
                }

                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                if dt > 0.0 {
                    self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
                }

                self.frame_count += 1;

                input::process_debug_hotkeys(self);

                scene::process_pending_load(self);

                input::update_camera_and_picking(self, dt, egui_consumed);

                let should_screenshot = self.pending_screenshot_path.is_some()
                    && self
                        .exit_after_frames
                        .is_some_and(|n| self.frame_count >= n);

                let (paint_jobs, textures_delta, screen_descriptor) = ui::render(self, dt);

                scene::process_post_ui_ops(self);

                render::frame(
                    self,
                    event_loop,
                    paint_jobs,
                    textures_delta,
                    screen_descriptor,
                    should_screenshot,
                    if self.freeze_time { 0.0 } else { dt },
                );
            }
            _ => {}
        }

        if self.pending_terrain_pipeline_rebuild {
            self.pending_terrain_pipeline_rebuild = false;
            if let Some(ctx) = self.ctx.as_ref() {
                let device = ctx.device.clone();
                let queue = ctx.queue.clone();
                // The 3D pipeline renders to the sRGB render target view, not the
                // non-sRGB surface view used by egui. Pass the same format as the
                // initial build in `resumed()`.
                let render_target_format = ctx.render_target_format();
                let width = ctx.config.width;
                let height = ctx.config.height;
                self.rebuild_pipeline_for_terrain_if_needed(
                    &device,
                    &queue,
                    render_target_format,
                    width,
                    height,
                );
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
