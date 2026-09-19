//! Aether Engine Launcher
//!
//! Thin orchestration layer. Builds the Scheduler from passes,
//! injects per-frame data via `RenderFrame`, and delegates execution.

mod cli;
mod handler;
mod input;
mod ready;
mod render;
mod scene;
mod ui;

use aether_engine::{
    asset::{registry::BuiltinMeshRegistry, texture_cache::GpuTextureCache, AssetManager},
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
    terrain::TerrainGeometry,
};
use cli::CliArgs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use winit::event_loop::{ControlFlow, EventLoop};

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
    ready_signal: ready::ReadySignal,
    pub(crate) window: Option<Arc<winit::window::Window>>,
    pub(crate) ctx: Option<RenderContext>,
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_winit_state: Option<egui_winit::State>,
    pub(crate) egui_renderer: Option<egui_wgpu::Renderer>,
    pub(crate) input: InputManager,
    pub(crate) camera: FlyCamera,
    pub(crate) mesh_registry: BuiltinMeshRegistry,
    pub(crate) asset_manager: AssetManager,
    pub(crate) texture_cache: Option<GpuTextureCache>,
    pub(crate) scheduler: Option<Scheduler>,
    pub(crate) has_terrain_pipeline: bool,
    pub(crate) terrain_geometry: Option<Arc<std::sync::RwLock<TerrainGeometry>>>,
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
    /// GPU adapter description, displayed in the editor overlay.
    pub(crate) gpu_info: String,
    /// Whether the GPU timer is supported, displayed in the editor overlay.
    pub(crate) gpu_timer_supported: bool,
}

impl App {
    fn new(cli: CliArgs) -> Self {
        let debug_mode = cli.debug_mode.unwrap_or_default();

        let no_gui_overlay = cli.no_gui_overlay;
        let exit_after_frames = cli.exit_after_frames;
        let freeze_time = cli.freeze_time;

        Self {
            ready_signal: ready::ReadySignal::from_env(),
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
            texture_cache: None,
            scheduler: None,
            has_terrain_pipeline: false,
            terrain_geometry: None,
            gpu_timer: None,
            ibl_resources: None,
            scene_entries: scene::discover_scenes(),
            state: LauncherState::Menu,
            pending_load: None,
            show_overlay: false,
            fullscreen_3d: false,
            pending_select_entity: None,
            debug_mode,
            ssao_enabled: cli.ssao_enabled,
            shadow_enabled: true,
            ibl_enabled: true,
            ssr_enabled: cli.ssr_enabled,
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
            gpu_info: String::new(),
            gpu_timer_supported: false,
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
        output_format: wgpu::TextureFormat,
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
        info!(
            "rebuilding pipeline: has_terrain={} current_has_terrain={}",
            has_terrain, self.has_terrain_pipeline
        );
        match crate::pipeline::build_pipeline(
            device,
            queue,
            self.texture_cache.as_ref().unwrap(),
            output_format,
            wgpu::TextureFormat::Depth32Float,
            width,
            height,
            has_terrain,
        ) {
            Ok((scheduler, ibl_resources)) => {
                info!("rebuilt pipeline with passes: {:?}", scheduler.pass_names());
                self.scheduler = Some(scheduler);
                self.ibl_resources = Some(ibl_resources);
                self.has_terrain_pipeline = has_terrain;
                self.gpu_timer = aether_engine::renderer::gpu_timer::GpuTimer::new(
                    device,
                    &self.scheduler.as_ref().unwrap().pass_names(),
                );
            }
            Err(err) => {
                tracing::error!("Failed to rebuild render pipeline: {}", err);
            }
        }
    }
}

fn set_working_dir_to_project_root() {
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(std::path::PathBuf::from);
        while let Some(current) = dir {
            if current.join("Cargo.toml").is_file() && current.join("assets").is_dir() {
                if let Err(e) = std::env::set_current_dir(&current) {
                    eprintln!(
                        "Warning: failed to set working directory to {:?}: {}",
                        current, e
                    );
                }
                return;
            }
            dir = current.parent().map(std::path::PathBuf::from);
        }
    }
}

pub fn run() {
    set_working_dir_to_project_root();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    info!("Aether Engine Launcher starting...");

    let cli = cli::parse_env_or_exit();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(cli);
    event_loop.run_app(&mut app).expect("Event loop error");
}
