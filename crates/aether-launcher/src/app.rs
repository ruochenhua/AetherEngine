//! Aether Engine Launcher
//!
//! Thin orchestration layer. Builds the Scheduler from passes,
//! injects per-frame data via `RenderFrame`, and delegates execution.

mod cli;
mod input;
mod render;
mod scene;
mod ui;

use aether_engine::{
    asset::{registry::BuiltinMeshRegistry, AssetManager},
    ecs::{Entity, World},
    input::InputManager,
    renderer::{
        camera::FlyCamera,
        context::RenderContext,
        gizmo::GizmoHandle,
        ibl::IblResources,
        light::LightingUniforms,
        passes::{fxaa::FxaaQuality, tone_mapping::ToneMappingMode},
        scheduler::Scheduler,
    },
    scene::loader::SceneLoader,
};
use cli::CliArgs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::{WindowAttributes, WindowId},
};

// ── Launcher state ──────────────────────────────────────────────────

#[allow(clippy::large_enum_variant)]
pub(crate) enum LauncherState {
    Menu,
    Running {
        world: World,
        lighting: LightingUniforms,
    },
}

pub(crate) struct SceneEntry {
    pub(crate) name: String,
    pub(crate) path: std::path::PathBuf,
}

// ── App ─────────────────────────────────────────────────────────────

pub(crate) struct App {
    pub(crate) window: Option<Arc<winit::window::Window>>,
    pub(crate) ctx: Option<RenderContext>,
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_winit_state: Option<egui_winit::State>,
    pub(crate) egui_renderer: Option<egui_wgpu::Renderer>,
    pub(crate) input: InputManager,
    pub(crate) camera: FlyCamera,
    pub(crate) mesh_registry: BuiltinMeshRegistry,
    pub(crate) asset_manager: AssetManager,
    pub(crate) scheduler: Option<Scheduler>,
    pub(crate) has_terrain_pipeline: bool,
    pub(crate) gpu_timer: Option<aether_engine::renderer::gpu_timer::GpuTimer>,
    pub(crate) ibl_resources: Option<IblResources>,
    pub(crate) scene_entries: Vec<SceneEntry>,
    pub(crate) state: LauncherState,
    pub(crate) pending_load: Option<usize>,
    pub(crate) show_overlay: bool,
    pub(crate) fullscreen_3d: bool,
    pub(crate) pending_select_entity: Option<Entity>,
    pub(crate) debug_mode: i32,
    pub(crate) ssao_enabled: bool,
    pub(crate) shadow_enabled: bool,
    pub(crate) ibl_enabled: bool,
    pub(crate) ssr_enabled: bool,
    pub(crate) tone_mapping_mode: ToneMappingMode,
    pub(crate) bloom_enabled: bool,
    pub(crate) bloom_threshold: f32,
    pub(crate) bloom_intensity: f32,
    pub(crate) fxaa_enabled: bool,
    pub(crate) fxaa_quality: FxaaQuality,
    pub(crate) fxaa_edge_threshold: Option<f32>,
    pub(crate) ssao_radius: f32,
    pub(crate) ssao_bias: f32,
    pub(crate) ssao_intensity: f32,
    pub(crate) ssr_debug_mode: u32,
    pub(crate) gizmo_drag_axis: Option<GizmoHandle>,
    /// Set to true when egui consumes a mouse-related window event.
    /// Cleared at the start of each RedrawRequested.
    pub(crate) egui_consumed_pointer: bool,
    pub(crate) pending_new_scene: bool,
    pub(crate) pending_open_dialog: bool,
    pub(crate) pending_import_dialog: bool,
    pub(crate) pending_save_dialog: bool,
    pub(crate) pending_add_cube: bool,
    pub(crate) pending_add_sphere: bool,
    pub(crate) pending_add_terrain: bool,
    pub(crate) pending_add_water: bool,
    pub(crate) pending_despawn_entity: Option<Entity>,
    pub(crate) pending_terrain_pipeline_rebuild: bool,
    pub(crate) undo_stack: Vec<crate::inspector::EditorCommand>,
    pub(crate) redo_stack: Vec<crate::inspector::EditorCommand>,
    pub(crate) gizmo_drag_start_transform: Option<aether_engine::ecs::components::Transform>,
    pub(crate) last_frame_time: std::time::Instant,
    pub(crate) scroll_input: f32,
    pub(crate) fps: f32,
    pub(crate) frame_count: u32,
    pub(crate) screenshot_taken: bool,
    pub(crate) pending_screenshot_path: Option<PathBuf>,
    pub(crate) screenshot_buffer: Option<wgpu::Buffer>,
    pub(crate) screenshot_bytes_per_row: u32,
    pub(crate) cli: CliArgs,
    pub(crate) no_gui_overlay: bool,
    pub(crate) exit_after_frames: Option<u32>,
    pub(crate) freeze_time: bool,
    pub(crate) frame_counter: u32,
}

impl App {
    fn new(cli: CliArgs) -> Self {
        let mut debug_mode = 0;
        if let Some(dm) = cli.debug_mode {
            debug_mode = dm;
        }

        let no_gui_overlay = cli.no_gui_overlay;
        let exit_after_frames = cli.exit_after_frames;
        let freeze_time = cli.freeze_time;

        Self {
            window: None,
            ctx: None,
            egui_ctx: egui::Context::default(),
            egui_winit_state: None,
            egui_renderer: None,
            input: InputManager::new(),
            camera: FlyCamera {
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
            },
            mesh_registry: BuiltinMeshRegistry::new(),
            asset_manager: AssetManager::new(),
            scheduler: None,
            has_terrain_pipeline: false,
            gpu_timer: None,
            ibl_resources: None,
            scene_entries: scene::discover_scenes(),
            state: LauncherState::Menu,
            pending_load: None,
            show_overlay: false,
            fullscreen_3d: false,
            pending_select_entity: None,
            debug_mode,
            ssao_enabled: true,
            shadow_enabled: true,
            ibl_enabled: true,
            ssr_enabled: false,
            tone_mapping_mode: ToneMappingMode::ACES,
            bloom_enabled: true,
            bloom_threshold: 1.0,
            bloom_intensity: 0.5,
            fxaa_enabled: true,
            fxaa_quality: FxaaQuality::High,
            fxaa_edge_threshold: None,
            ssao_radius: 0.5f32,
            ssao_bias: 0.025f32,
            ssao_intensity: 1.5f32,
            ssr_debug_mode: 0,
            gizmo_drag_axis: None,
            egui_consumed_pointer: false,
            pending_new_scene: false,
            pending_open_dialog: false,
            pending_import_dialog: false,
            pending_save_dialog: false,
            pending_add_cube: false,
            pending_add_sphere: false,
            pending_add_terrain: false,
            pending_add_water: false,
            pending_despawn_entity: None,
            pending_terrain_pipeline_rebuild: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            gizmo_drag_start_transform: None,
            last_frame_time: std::time::Instant::now(),
            scroll_input: 0.0,
            fps: 0.0,
            frame_count: 0,
            screenshot_taken: false,
            pending_screenshot_path: cli.screenshot.clone(),
            screenshot_buffer: None,
            screenshot_bytes_per_row: 0,
            cli,
            no_gui_overlay,
            exit_after_frames,
            freeze_time,
            frame_counter: 0,
        }
    }

    /// Pop the top command from the undo stack and revert it.
    fn apply_undo(&mut self) {
        let Some(cmd) = self.undo_stack.pop() else {
            return;
        };
        if let LauncherState::Running { ref mut world, .. } = self.state {
            let redo = crate::inspector::apply_undo(world, &cmd);
            self.redo_stack.push(redo);
        }
    }

    /// Pop the top command from the redo stack and re-apply it.
    fn apply_redo(&mut self) {
        let Some(cmd) = self.redo_stack.pop() else {
            return;
        };
        if let LauncherState::Running { ref mut world, .. } = self.state {
            let undo = crate::inspector::apply_undo(world, &cmd);
            self.undo_stack.push(undo);
        }
    }

    /// Rebuild the render pipeline if the terrain presence in the world has
    /// changed relative to the currently scheduled pipeline.
    fn rebuild_pipeline_for_terrain_if_needed(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        let has_terrain = if let LauncherState::Running { ref world, .. } = self.state {
            world
                .query::<&aether_engine::ecs::components::Terrain>()
                .iter()
                .next()
                .is_some()
        } else {
            return;
        };
        if has_terrain == self.has_terrain_pipeline {
            return;
        }
        let (scheduler, ibl_resources) = crate::pipeline::build_pipeline(
            device,
            queue,
            surface_format,
            wgpu::TextureFormat::Depth32Float,
            width,
            height,
            has_terrain,
        );
        self.scheduler = Some(scheduler);
        self.ibl_resources = Some(ibl_resources);
        self.has_terrain_pipeline = has_terrain;
        self.gpu_timer = aether_engine::renderer::gpu_timer::GpuTimer::new(
            device,
            &self.scheduler.as_ref().unwrap().pass_names(),
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Aether Engine Launcher")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("Failed to create window"),
        );

        let ctx = pollster::block_on(RenderContext::new(&window));

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

        let surface_format = ctx.surface_format();
        let depth_format = wgpu::TextureFormat::Depth32Float;

        // Build render pipeline via helper. Terrain presence is determined by
        // the scene; the empty default scene has no terrain.
        let has_terrain = false;
        let (scheduler, ibl_resources) = crate::pipeline::build_pipeline(
            &ctx.device,
            &ctx.queue,
            surface_format,
            depth_format,
            ctx.config.width,
            ctx.config.height,
            has_terrain,
        );
        self.has_terrain_pipeline = has_terrain;
        self.ibl_resources = Some(ibl_resources);

        // Initialize GPU timer for the performance panel.
        self.gpu_timer =
            aether_engine::renderer::gpu_timer::GpuTimer::new(&ctx.device, &scheduler.pass_names());
        info!(
            "GPU timer initialized: supported={}",
            self.gpu_timer.as_ref().is_some_and(|t| t.supported)
        );

        // Start with an empty scene (default camera + lighting + a default cube)
        let mut world = World::new();
        let lighting = SceneLoader::new_empty(&mut world);
        crate::pipeline::spawn_default_cube(&ctx.device, &self.mesh_registry, &mut world);
        self.state = LauncherState::Running { world, lighting };
        self.show_overlay = true;

        // Auto-open scene if --scene is provided
        scene::open_cli_scene(self, &ctx);

        info!("Discovered {} scenes", self.scene_entries.len());

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
                scheduler.set_bloom_screen_size(size.width, size.height);
                scheduler.set_ssao_screen_size(size.width, size.height);
                scheduler.set_ao_blur_screen_size(size.width, size.height);
                scheduler.set_ssr_screen_size(size.width, size.height);
                scheduler.rebuild(&ctx.device, size.width, size.height);
                self.gpu_timer = aether_engine::renderer::gpu_timer::GpuTimer::new(
                    &ctx.device,
                    &scheduler.pass_names(),
                );
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
                let surface_format = ctx.surface_format();
                let width = ctx.config.width;
                let height = ctx.config.height;
                self.rebuild_pipeline_for_terrain_if_needed(
                    &device,
                    &queue,
                    surface_format,
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

// ── Main ────────────────────────────────────────────────────────────

pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    info!("Aether Engine Launcher starting...");

    let all_args: Vec<String> = std::env::args().collect();
    let cli = cli::parse_args(&all_args);

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(cli);
    event_loop.run_app(&mut app).expect("Event loop error");
}
