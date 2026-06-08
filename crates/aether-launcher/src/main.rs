//! Aether Engine Launcher
//!
//! Thin orchestration layer. Builds the Scheduler from passes,
//! injects per-frame data via `RenderFrame`, and delegates execution.

use aether_engine::{
    asset::registry::BuiltinMeshRegistry,
    ecs::components::{Selected, Transform},
    ecs::{Entity, World},
    input::InputManager,
    renderer::renderable::MaterialUniform,
    renderer::{
        camera::FlyCamera,
        context::RenderContext,
        extract::extract_render_batches,
        frame::RenderFrame,
        gizmo::{apply_drag, build_transform_gizmo, detect_hover, selected_entity_transform, GizmoHandle},
        ibl::{IblResources},
        light::LightingUniforms,
        picking::{pick_entity, screen_ray},
        passes::{
            composite::CompositePass,
            debug::DebugLinePass,
            gbuffer::GBufferPass,
            lighting::LightingPass,
            shadow::ShadowPass,
            ssao::SSAOPass,
            ssr::SSRPass,
        },
        scheduler::{PipelineBuilder, Scheduler},
    },
    scene::loader::SceneLoader,
};
use std::sync::Arc;
use std::path::PathBuf;
use tracing::{error, info};
use winit::{
    application::ApplicationHandler,
    event::{WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::{WindowAttributes, WindowId},
};

// ── Undo/Redo commands ──────────────────────────────────────────────

/// A reversible editor action.
#[derive(Clone)]
enum EditorCommand {
    /// Restore a Transform to a previous value.
    Transform {
        entity: Entity,
        old_transform: Transform,
    },
    /// Restore a Material to a previous value.
    Material {
        entity: Entity,
        old_material: aether_engine::renderer::renderable::MaterialUniform,
    },
}

// ── CLI parsing ─────────────────────────────────────────────────────

struct CliArgs {
    scene: Option<String>,
    screenshot: Option<PathBuf>,
    exit_after_frames: Option<u32>,
    no_gui_overlay: bool,
    debug_mode: Option<i32>,
}

fn parse_args(args: &[String]) -> CliArgs {
    let mut cli = CliArgs {
        scene: None,
        screenshot: None,
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scene" => {
                i += 1;
                if i < args.len() {
                    cli.scene = Some(args[i].clone());
                }
            }
            "--screenshot" => {
                i += 1;
                if i < args.len() {
                    cli.screenshot = Some(PathBuf::from(&args[i]));
                }
            }
            "--exit-after-frames" => {
                i += 1;
                if i < args.len() {
                    cli.exit_after_frames = args[i].parse().ok();
                }
            }
            "--no-gui-overlay" => {
                cli.no_gui_overlay = true;
            }
            "--debug-mode" => {
                i += 1;
                if i < args.len() {
                    cli.debug_mode = args[i].parse().ok();
                }
            }
            _ => {}
        }
        i += 1;
    }
    cli
}

// ── Screenshot helpers ──────────────────────────────────────────────

fn screenshot_buffer_size(width: u32, height: u32) -> (u64, u32) {
    let bytes_per_row = ((width * 4 + 255) / 256) * 256;
    let buffer_size = bytes_per_row as u64 * height as u64;
    (buffer_size, bytes_per_row)
}

fn save_screenshot(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let data = buffer.slice(..).get_mapped_range();

    // BGRA -> RGBA and remove row padding
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row_start = (y * bytes_per_row) as usize;
        for x in 0..width {
            let idx = row_start + (x * 4) as usize;
            rgba.push(data[idx + 2]); // R
            rgba.push(data[idx + 1]); // G
            rgba.push(data[idx]);     // B
            rgba.push(data[idx + 3]); // A
        }
    }
    drop(data);
    buffer.unmap();

    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;
    img.save(path)?;

    Ok(())
}

// ── Launcher state ──────────────────────────────────────────────────

enum LauncherState {
    Menu,
    Running {
        world: World,
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

// ── App ─────────────────────────────────────────────────────────────

struct App {
    window: Option<Arc<winit::window::Window>>,
    ctx: Option<RenderContext>,
    egui_ctx: egui::Context,
    egui_winit_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
    input: InputManager,
    camera: FlyCamera,
    mesh_registry: BuiltinMeshRegistry,
    scheduler: Option<Scheduler>,
    ibl_resources: Option<IblResources>,
    scene_entries: Vec<SceneEntry>,
    state: LauncherState,
    pending_load: Option<usize>,
    show_overlay: bool,
    fullscreen_3d: bool,
    pending_select_entity: Option<Entity>,
    debug_mode: i32,
    ssao_enabled: bool,
    shadow_enabled: bool,
    ibl_enabled: bool,
    ssr_enabled: bool,
    ssao_radius: f32,
    ssao_bias: f32,
    ssao_intensity: f32,
    ssr_debug_mode: u32,
    gizmo_drag_axis: Option<GizmoHandle>,
    /// Set to true when egui consumes a mouse-related window event.
    /// Cleared at the start of each RedrawRequested.
    egui_consumed_pointer: bool,
    pending_new_scene: bool,
    pending_open_dialog: bool,
    pending_import_dialog: bool,
    pending_save_dialog: bool,
    pending_add_cube: bool,
    pending_add_sphere: bool,
    pending_despawn_entity: Option<Entity>,
    undo_stack: Vec<EditorCommand>,
    redo_stack: Vec<EditorCommand>,
    gizmo_drag_start_transform: Option<Transform>,
    last_frame_time: std::time::Instant,
    scroll_input: f32,
    fps: f32,
    frame_count: u32,
    screenshot_taken: bool,
    pending_screenshot_path: Option<PathBuf>,
    screenshot_buffer: Option<wgpu::Buffer>,
    screenshot_bytes_per_row: u32,
    cli: CliArgs,
    no_gui_overlay: bool,
    exit_after_frames: Option<u32>,
}

impl App {
    fn new(cli: CliArgs) -> Self {
        let mut debug_mode = 0;
        if let Some(dm) = cli.debug_mode {
            debug_mode = dm;
        }

        let no_gui_overlay = cli.no_gui_overlay;
        let exit_after_frames = cli.exit_after_frames;

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
            scheduler: None,
            ibl_resources: None,
            scene_entries: discover_scenes(),
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
            ssao_radius: 0.15f32,
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
            pending_despawn_entity: None,
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
        }
    }

    /// Pop the top command from the undo stack and revert it.
    fn apply_undo(&mut self) {
        let Some(cmd) = self.undo_stack.pop() else { return };
        if let LauncherState::Running { ref mut world, .. } = self.state {
            match cmd.clone() {
                EditorCommand::Transform { entity, old_transform } => {
                    if let Ok(current) = world.query_one_mut::<&mut Transform>(entity) {
                        let current_copy = current.clone();
                        *current = old_transform;
                        self.redo_stack.push(EditorCommand::Transform {
                            entity,
                            old_transform: current_copy,
                        });
                    }
                }
                EditorCommand::Material { entity, old_material } => {
                    if let Ok(current) = world.query_one_mut::<&mut aether_engine::renderer::renderable::MaterialUniform>(entity) {
                        let current_copy = *current;
                        *current = old_material;
                        self.redo_stack.push(EditorCommand::Material {
                            entity,
                            old_material: current_copy,
                        });
                    }
                }
            }
        }
    }

    /// Pop the top command from the redo stack and re-apply it.
    fn apply_redo(&mut self) {
        let Some(cmd) = self.redo_stack.pop() else { return };
        if let LauncherState::Running { ref mut world, .. } = self.state {
            match cmd.clone() {
                EditorCommand::Transform { entity, old_transform } => {
                    if let Ok(current) = world.query_one_mut::<&mut Transform>(entity) {
                        let current_copy = current.clone();
                        *current = old_transform;
                        self.undo_stack.push(EditorCommand::Transform {
                            entity,
                            old_transform: current_copy,
                        });
                    }
                }
                EditorCommand::Material { entity, old_material } => {
                    if let Ok(current) = world.query_one_mut::<&mut aether_engine::renderer::renderable::MaterialUniform>(entity) {
                        let current_copy = *current;
                        *current = old_material;
                        self.undo_stack.push(EditorCommand::Material {
                            entity,
                            old_material: current_copy,
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Camera sync helpers
// ---------------------------------------------------------------------------

/// Read camera state from the first `(Transform, Camera)` entity.
fn read_camera_from_world(
    world: &aether_engine::ecs::World,
) -> Option<(glam::Vec3, f32, f32, f32)> {
    for (transform, cam) in world
        .query::<(
            &aether_engine::ecs::components::Transform,
            &aether_engine::ecs::components::Camera,
        )>()
        .iter()
    {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(glam::EulerRot::YXZ);
        return Some((transform.translation, yaw, pitch, cam.fov));
    }
    None
}

/// Write camera state to the first `(Transform, Camera)` entity.
fn write_camera_to_world(
    camera: &aether_engine::renderer::camera::FlyCamera,
    world: &mut aether_engine::ecs::World,
) {
    let mut target = None;
    for (entity, _) in world
        .query::<(
            aether_engine::ecs::Entity,
            (
                &aether_engine::ecs::components::Transform,
                &aether_engine::ecs::components::Camera,
            ),
        )>()
        .iter()
    {
        target = Some(entity);
        break;
    }
    if let Some(entity) = target {
        let _ = world.insert(
            entity,
            (
                aether_engine::ecs::components::Transform {
                    translation: camera.position,
                    rotation: glam::Quat::from_euler(
                        glam::EulerRot::YXZ,
                        camera.yaw,
                        camera.pitch,
                        0.0,
                    ),
                    scale: glam::Vec3::ONE,
                },
                aether_engine::ecs::components::Camera {
                    fov: camera.fov,
                    ..Default::default()
                },
            ),
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop.create_window(
                WindowAttributes::default()
                    .with_title("Aether Engine Launcher")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
            ).expect("Failed to create window"),
        );

        let ctx = pollster::block_on(RenderContext::new(&window));

        let viewport_id = self.egui_ctx.viewport_id();
        let egui_winit_state =
            egui_winit::State::new(self.egui_ctx.clone(), viewport_id, &window, None, None, None);
        let egui_renderer =
            egui_wgpu::Renderer::new(&ctx.device, ctx.surface_format(), egui_wgpu::RendererOptions::default());

        let surface_format = ctx.surface_format();
        let depth_format = wgpu::TextureFormat::Depth32Float;

        // Create IBL resources from HDR environment map
        let ibl_resources = IblResources::generate(
            &ctx.device,
            Some(&ctx.queue),
            &aether_engine::renderer::ibl::IblConfig {
                environment_path: Some("assets/hdr/newport_loft.hdr".into()),
                ..Default::default()
            },
        );

        // Build scheduler: validates the pass graph, allocates textures, resolves passes.
        let mut scheduler = PipelineBuilder::new()
            .add(ShadowPass::new(&ctx.device))
            .add(GBufferPass::new(&ctx.device))
            .add(SSAOPass::new(&ctx.device))
            .add(LightingPass::new_with_ibl(&ctx.device, surface_format, &ibl_resources))
            .add(SSRPass::new(&ctx.device))
            .add(CompositePass::new(&ctx.device, surface_format))
            .add(DebugLinePass::new(&ctx.device, surface_format, depth_format))
            .build(&ctx.device, ctx.config.width, ctx.config.height);

        scheduler.set_ssao_screen_size(ctx.config.width, ctx.config.height);
        scheduler.set_ssr_screen_size(ctx.config.width, ctx.config.height);

        // Start with an empty scene (default camera + lighting + a default cube)
        let mut world = World::new();
        let lighting = SceneLoader::new_empty(&mut world);
        // Spawn a default cube so there's something to pick right away
        if let Some(cpu_mesh) = self.mesh_registry.get("cube") {
            let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
                &ctx.device, &cpu_mesh,
            ));
            let entity = world.spawn((
                aether_engine::ecs::components::Transform::default(),
                aether_engine::ecs::components::MeshHandle::new(gpu_mesh, "cube"),
                aether_engine::renderer::renderable::MaterialUniform {
                    albedo: [0.8, 0.3, 0.2, 1.0],
                    roughness: 0.5,
                    metallic: 0.0,
                    _pad: [0.0, 0.0],
                },
                aether_engine::ecs::components::Visibility::default(),
            ));
            let _ = world.insert(entity, (aether_engine::ecs::components::Selected,));
        }
        self.state = LauncherState::Running { world, lighting };
        self.show_overlay = true;

        // Auto-open scene if --scene is provided
        if let Some(scene_path) = &self.cli.scene {
            let path = std::path::PathBuf::from(scene_path);
            if let LauncherState::Running { ref mut world, ref mut lighting } = self.state {
                match SceneLoader::open_scene(&path, &ctx.device, &self.mesh_registry, world) {
                    Ok(new_lighting) => {
                        *lighting = new_lighting;
                        if let Some((pos, yaw, pitch, fov)) = read_camera_from_world(world) {
                            self.camera.position = pos;
                            self.camera.yaw = yaw;
                            self.camera.pitch = pitch;
                            self.camera.fov = fov;
                            self.camera.active = false;
                        }
                    }
                    Err(e) => {
                        error!("Open scene error: {:?}", e);
                        std::process::exit(1);
                    }
                }
            }
        }

        info!("Discovered {} scenes", self.scene_entries.len());

        self.window = Some(window);
        self.ctx = Some(ctx);
        self.egui_winit_state = Some(egui_winit_state);
        self.egui_renderer = Some(egui_renderer);
        self.scheduler = Some(scheduler);
        self.ibl_resources = Some(ibl_resources);
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

        let window = self.window.as_ref().unwrap();
        let ctx = self.ctx.as_mut().unwrap();
        let scheduler = self.scheduler.as_mut().unwrap();
        let egui_winit_state = self.egui_winit_state.as_mut().unwrap();
        let egui_renderer = self.egui_renderer.as_mut().unwrap();

        let egui_response = egui_winit_state.on_window_event(window, &event);
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
            WindowEvent::Resized(size)
                if size.width > 0 && size.height > 0 =>
            {
                ctx.resize(size.width, size.height);
                scheduler.rebuild(&ctx.device, size.width, size.height);
                scheduler.set_ssao_screen_size(size.width, size.height);
                scheduler.set_ssr_screen_size(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                // Reset per-frame egui pointer consumption flag.
                let egui_consumed = self.egui_consumed_pointer;
                self.egui_consumed_pointer = false;

                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                if dt > 0.0 {
                    self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
                }

                self.frame_count += 1;

                if self.input.key_pressed(KeyCode::Digit0) { self.debug_mode = 0; }
                if self.input.key_pressed(KeyCode::Digit1) { self.debug_mode = 1; }
                if self.input.key_pressed(KeyCode::Digit2) { self.debug_mode = 2; }
                if self.input.key_pressed(KeyCode::Digit3) { self.debug_mode = 3; }
                if self.input.key_pressed(KeyCode::Digit4) { self.debug_mode = 4; }
                if self.input.key_pressed(KeyCode::Digit5) { self.debug_mode = 5; }
                if self.input.key_pressed(KeyCode::Digit6) { self.debug_mode = 6; }
                if self.input.key_pressed(KeyCode::Digit7) { self.debug_mode = 7; }
                if self.input.key_pressed(KeyCode::Digit8) { self.debug_mode = 8; }
                if self.input.key_pressed(KeyCode::Digit9) { self.debug_mode = 9; }
                if self.input.key_pressed(KeyCode::F1) { self.debug_mode = 10; }
                if self.input.key_pressed(KeyCode::F2) { self.debug_mode = 11; }
                if self.input.key_pressed(KeyCode::F3) { self.debug_mode = 12; }
                if self.input.key_pressed(KeyCode::F4) { self.debug_mode = 13; }
                if self.input.key_pressed(KeyCode::F5) { self.debug_mode = 14; }
                if self.input.key_pressed(KeyCode::F6) {
                    self.ssr_debug_mode = (self.ssr_debug_mode + 1) % 10;
                }

                if let Some(idx) = self.pending_load.take() {
                    let entry = &self.scene_entries[idx];
                    if let LauncherState::Running { ref mut world, ref mut lighting } = self.state {
                        match SceneLoader::open_scene(&entry.path, &ctx.device, &self.mesh_registry, world) {
                            Ok(new_lighting) => {
                                *lighting = new_lighting;
                                if let Some((pos, yaw, pitch, fov)) = read_camera_from_world(world) {
                                    self.camera.position = pos;
                                    self.camera.yaw = yaw;
                                    self.camera.pitch = pitch;
                                    self.camera.fov = fov;
                                    self.camera.active = false;
                                }
                                self.show_overlay = false;
                            }
                            Err(e) => {
                                error!("Open scene error: {:?}", e);
                            }
                        }
                    }
                }

                // Camera update (only when pointer is not over egui UI)
                if !egui_consumed && matches!(self.state, LauncherState::Running { .. }) {
                    let (dx, dy) = self.input.mouse_delta();
                    self.camera.update(dt, dx, dy, self.scroll_input, &self.input);
                    self.scroll_input = 0.0;
                }

                // Delete selected entity on Delete key
                if self.input.key_pressed(KeyCode::Delete) {
                    if let LauncherState::Running { ref mut world, .. } = self.state {
                        if let Some((entity, _)) = selected_entity_transform(world) {
                            self.pending_despawn_entity = Some(entity);
                        }
                    }
                }

                // Picking + Gizmo interaction (only in Running state, and not over UI)
                if !egui_consumed {
                    if let LauncherState::Running { ref mut world, .. } = self.state {
                        let width = ctx.config.width as f32;
                        let height = ctx.config.height as f32;
                        let (mx, my) = self.input.mouse_position();
                        let view = self.camera.view_matrix();
                        let proj = self.camera.projection_matrix(width / height);
                        let mouse_pressed = self.input.mouse_pressed(winit::event::MouseButton::Left)
                            && !self.input.alt_held();
                        let mouse_held = self.input.mouse_held(winit::event::MouseButton::Left)
                            && !self.input.alt_held();
                        let mouse_released = self.input.mouse_released(winit::event::MouseButton::Left);

                        // Gizmo drag handling
                        if let Some(axis) = self.gizmo_drag_axis {
                            if mouse_held {
                                let (dx, dy) = self.input.mouse_delta();
                                if let Some((entity, _)) = selected_entity_transform(world) {
                                    if let Ok(mut transform) = world.query_one_mut::<&mut Transform>(entity) {
                                        apply_drag(&mut transform, axis, glam::Vec2::new(dx, dy), view, proj, width, height, self.camera.position);
                                    }
                                }
                            }
                            if mouse_released {
                                // Record undo command if transform changed during drag
                                if let Some((entity, _)) = selected_entity_transform(world) {
                                    if let Some(old_transform) = self.gizmo_drag_start_transform.take() {
                                        if let Ok(transform) = world.query_one_mut::<&mut Transform>(entity) {
                                            if *transform != old_transform {
                                                self.undo_stack.push(EditorCommand::Transform {
                                                    entity,
                                                    old_transform,
                                                });
                                                self.redo_stack.clear();
                                            }
                                        }
                                    }
                                }
                                self.gizmo_drag_axis = None;
                            }
                        } else if mouse_pressed {
                            // Check gizmo hover before picking
                            if let Some((_, transform)) = selected_entity_transform(world) {
                                if let Some(hovered) = detect_hover(&transform, view, proj, mx, my, width, height) {
                                    self.gizmo_drag_axis = Some(hovered);
                                    self.gizmo_drag_start_transform = Some(transform.clone());
                                } else {
                                    // Not hovering gizmo: perform picking
                                    let ray = screen_ray(mx, my, width, height, view, proj, self.camera.position);
                                    pick_entity(world, &ray);
                                }
                            } else {
                                // No selection: perform picking
                                let ray = screen_ray(mx, my, width, height, view, proj, self.camera.position);
                                pick_entity(world, &ray);
                            }
                        }
                    }
                }

                let should_screenshot = self.pending_screenshot_path.is_some()
                    && self.exit_after_frames.map_or(false, |n| self.frame_count >= n);

                // egui UI
                let raw_input = egui_winit_state.take_egui_input(window);
                let skip_egui = self.no_gui_overlay
                    && matches!(self.state, LauncherState::Running { .. });

                let (paint_jobs, textures_delta, screen_descriptor) = if skip_egui {
                    let sd = egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [ctx.config.width, ctx.config.height],
                        pixels_per_point: window.scale_factor() as f32,
                    };
                    (vec![], egui::TexturesDelta::default(), sd)
                } else {
                    // Extract mutable references needed by the egui closure
                    let scroll_input = &mut self.scroll_input;
                    let state = &self.state;
                    let scene_entries = &self.scene_entries;
                    let pending_load = &mut self.pending_load;
                    let camera = &self.camera;
                    let fps = self.fps;
                    let ssao_enabled = &mut self.ssao_enabled;
                    let shadow_enabled = &mut self.shadow_enabled;
                    let ibl_enabled = &mut self.ibl_enabled;
                    let ssr_enabled = &mut self.ssr_enabled;
                    let ssr_debug_mode = self.ssr_debug_mode;
                    let show_overlay = &mut self.show_overlay;
                    let fullscreen_3d = &mut self.fullscreen_3d;
                    let debug_mode = self.debug_mode;
                    let pending_select_entity = &mut self.pending_select_entity;

                    // Extract inspector data before UI (mutable borrow needed for editing)
                    #[derive(Clone)]
                    struct InspectorData {
                        entity: Entity,
                        transform: Transform,
                        material: MaterialUniform,
                        euler: [f32; 3], // [x, y, z] in radians (EulerRot::XYZ)
                    }
                    let mut inspector_data: Option<InspectorData> = None;
                    let mut selected_entity: Option<Entity> = None;
                    if let LauncherState::Running { ref world, .. } = self.state {
                        for (entity, transform, material, _) in world.query::<(Entity, &Transform, &MaterialUniform, &Selected)>().iter() {
                            let (ex, ey, ez) = transform.rotation.to_euler(glam::EulerRot::XYZ);
                            inspector_data = Some(InspectorData {
                                entity,
                                transform: transform.clone(),
                                material: *material,
                                euler: [ex, ey, ez],
                            });
                            selected_entity = Some(entity);
                            break;
                        }
                    }

                    // Extract hierarchy data (all mesh entities)
                    #[derive(Clone)]
                    struct HierarchyItem {
                        entity: Entity,
                        name: String,
                    }
                    let mut hierarchy_items: Vec<HierarchyItem> = Vec::new();
                    if let LauncherState::Running { ref world, .. } = self.state {
                        // Collect first, then sort by entity ID for stable ordering.
                        // This ensures display names remain tied to the same entity
                        // even if hecs query iteration order shifts after component edits.
                        let mut raw: Vec<(Entity, String)> = Vec::new();
                        for (entity, mesh_handle) in
                            world.query::<(Entity, &aether_engine::ecs::components::MeshHandle)>().iter()
                        {
                            raw.push((entity, mesh_handle.name.clone()));
                        }
                        raw.sort_by(|a, b| a.0.cmp(&b.0));

                        let mut name_counts: std::collections::HashMap<String, usize> =
                            std::collections::HashMap::new();
                        for (entity, base_name) in raw {
                            let count = name_counts.entry(base_name.clone()).or_insert(0);
                            let display_name = if *count == 0 {
                                base_name.clone()
                            } else {
                                format!("{} ({})", base_name, count)
                            };
                            *count += 1;
                            hierarchy_items.push(HierarchyItem {
                                entity,
                                name: display_name,
                            });
                        }
                    }

                    let egui_output = self.egui_ctx.run_ui(raw_input, |ui| {
                        *scroll_input +=
                            ui.ctx().input(|i| i.smooth_scroll_delta.y * 0.01);

                        match state {
                            LauncherState::Menu => {
                                egui::CentralPanel::default().show_inside(ui, |ui| {
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
                                                    *pending_load = Some(idx);
                                                }
                                            });
                                            ui.separator();
                                        }
                                    }
                                });
                            }
                            LauncherState::Running { world, .. } => {
                                if *show_overlay {
                                    // Top menu bar
                                    egui::Panel::top("menu_bar")
                                        .show_inside(ui, |ui| {
                                            egui::MenuBar::new().ui(ui, |ui| {
                                                ui.menu_button("File", |ui| {
                                                    if ui.button("New Scene").clicked() {
                                                        self.pending_new_scene = true;
                                                        ui.close();
                                                    }
                                                    if ui.button("Open Scene...").clicked() {
                                                        self.pending_open_dialog = true;
                                                        ui.close();
                                                    }
                                                    if ui.button("Import Scene...").clicked() {
                                                        self.pending_import_dialog = true;
                                                        ui.close();
                                                    }
                                                    if ui.button("Save Scene...").clicked() {
                                                        self.pending_save_dialog = true;
                                                        ui.close();
                                                    }
                                                });
                                                ui.menu_button("Create", |ui| {
                                                    if ui.button("Add Cube").clicked() {
                                                        self.pending_add_cube = true;
                                                        ui.close();
                                                    }
                                                    if ui.button("Add Sphere").clicked() {
                                                        self.pending_add_sphere = true;
                                                        ui.close();
                                                    }
                                                });
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    let label = if *fullscreen_3d { "⛶ Exit Full Screen" } else { "⛶ Full Screen" };
                                                    if ui.button(label).clicked() {
                                                        *fullscreen_3d = !*fullscreen_3d;
                                                    }
                                                });
                                            });
                                        });

                                    // Left hierarchy panel (hidden in fullscreen)
                                    if !*fullscreen_3d {
                                        egui::Panel::left("hierarchy")
                                            .default_size(200.0)
                                            .show_inside(ui, |ui| {
                                                ui.heading("Scene");
                                                ui.separator();
                                                egui::ScrollArea::vertical().show(ui, |ui| {
                                                    for item in &hierarchy_items {
                                                        ui.horizontal(|ui| {
                                                            let is_selected = selected_entity == Some(item.entity);
                                                            let label = egui::Button::new(&item.name)
                                                                .selected(is_selected)
                                                                .fill(if is_selected {
                                                                    ui.visuals().selection.bg_fill
                                                                } else {
                                                                    ui.visuals().widgets.inactive.weak_bg_fill
                                                                });
                                                            if ui.add(label).clicked() {
                                                                *pending_select_entity = Some(item.entity);
                                                            }
                                                            if ui.small_button("🗑").clicked() {
                                                                self.pending_despawn_entity = Some(item.entity);
                                                            }
                                                        });
                                                    }
                                                });
                                            });
                                    }

                                    // Right inspector panel (hidden in fullscreen)
                                    if !*fullscreen_3d {
                                        egui::Panel::right("inspector")
                                            .default_size(240.0)
                                            .show_inside(ui, |ui| {
                                                egui::ScrollArea::vertical().show(ui, |ui| {
                                                    // Inspector
                                                    if let Some(ref mut data) = inspector_data.as_mut() {
                                                        ui.heading("Inspector");
                                                        ui.label("Translation");
                                                        ui.horizontal(|ui| {
                                                            ui.label("X");
                                                            ui.add(egui::DragValue::new(&mut data.transform.translation.x).speed(0.1));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Y");
                                                            ui.add(egui::DragValue::new(&mut data.transform.translation.y).speed(0.1));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Z");
                                                            ui.add(egui::DragValue::new(&mut data.transform.translation.z).speed(0.1));
                                                        });
                                                        ui.label("Rotation (rad)");
                                                        ui.horizontal(|ui| {
                                                            ui.label("X");
                                                            ui.add(egui::DragValue::new(&mut data.euler[0]).speed(0.01));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Y");
                                                            ui.add(egui::DragValue::new(&mut data.euler[1]).speed(0.01));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Z");
                                                            ui.add(egui::DragValue::new(&mut data.euler[2]).speed(0.01));
                                                        });
                                                        ui.label("Scale");
                                                        ui.horizontal(|ui| {
                                                            ui.label("X");
                                                            ui.add(egui::DragValue::new(&mut data.transform.scale.x).speed(0.05));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Y");
                                                            ui.add(egui::DragValue::new(&mut data.transform.scale.y).speed(0.05));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("Z");
                                                            ui.add(egui::DragValue::new(&mut data.transform.scale.z).speed(0.05));
                                                        });

                                                        ui.label("Material");
                                                        ui.horizontal(|ui| {
                                                            ui.label("R");
                                                            ui.add(egui::DragValue::new(&mut data.material.albedo[0]).speed(0.01).range(0.0..=1.0));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("G");
                                                            ui.add(egui::DragValue::new(&mut data.material.albedo[1]).speed(0.01).range(0.0..=1.0));
                                                        });
                                                        ui.horizontal(|ui| {
                                                            ui.label("B");
                                                            ui.add(egui::DragValue::new(&mut data.material.albedo[2]).speed(0.01).range(0.0..=1.0));
                                                        });
                                                        ui.add(egui::Slider::new(&mut data.material.roughness, 0.0..=1.0).text("Roughness"));
                                                        ui.add(egui::Slider::new(&mut data.material.metallic, 0.0..=1.0).text("Metallic"));
                                                        ui.separator();
                                                    }

                                                    ui.heading("Scene Info");
                                                    ui.separator();
                                                    ui.label(format!("FPS: {:.1}", fps));
                                                    ui.label(format!(
                                                        "Frame: {:.2} ms",
                                                        dt * 1000.0
                                                    ));
                                                    ui.label(format!(
                                                        "Entities: {}",
                                                        world.len()
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
                                                    let mode_names = [
                                                        "Full", "Ambient", "Diffuse",
                                                        "Specular", "Normals", "NdotL",
                                                        "Shadow", "Direct", "IBL",
                                                        "Alpha(P)", "Alpha(N)",
                                                        "NDC(F2)", "EnvFix(F3)", "VDir(F4)",
                                                        "SSAO(F5)",
                                                    ];
                                                    let mode_idx =
                                                        debug_mode.clamp(0, 14) as usize;
                                                    ui.label(format!(
                                                        "Debug: [{}] {}",
                                                        mode_idx, mode_names[mode_idx]
                                                    ));
                                                    ui.label(format!(
                                                        "SSR Debug: {} (F6)",
                                                        ssr_debug_mode
                                                    ));
                                                    ui.separator();

                                                    ui.heading("Features");
                                                    ui.checkbox(ssao_enabled, "SSAO");
                                                    ui.checkbox(shadow_enabled, "Shadow Map");
                                                    ui.checkbox(ibl_enabled, "IBL");
                                                    ui.checkbox(ssr_enabled, "SSR");
                                                    ui.separator();

                                                    ui.heading("Input Debug");
                                                    let (mdx, mdy) = self.input.mouse_delta();
                                                    ui.label(format!("Mouse delta: ({:.1}, {:.1})", mdx, mdy));
                                                    ui.label(format!("Alt held: {}", self.input.alt_held()));
                                                    ui.label(format!("Left held: {}", self.input.mouse_held(winit::event::MouseButton::Left)));
                                                });
                                            });
                                    }
                                }

                                // Central panel: 3D viewport (transparent so 3D render shows through)
                                egui::CentralPanel::default()
                                    .frame(egui::Frame::NONE)
                                    .show_inside(ui, |_ui| {});
                            }
                        }
                    });

                    // Write back inspector changes
                    if let Some(ref data) = inspector_data {
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            // Transform
                            if let Ok(t) = world.query_one_mut::<&mut Transform>(data.entity) {
                                let mut tx = data.transform.clone();
                                tx.rotation = glam::Quat::from_euler(
                                    glam::EulerRot::XYZ,
                                    data.euler[0],
                                    data.euler[1],
                                    data.euler[2],
                                );
                                if *t != tx {
                                    self.undo_stack.push(EditorCommand::Transform {
                                        entity: data.entity,
                                        old_transform: t.clone(),
                                    });
                                    self.redo_stack.clear();
                                }
                                *t = tx;
                            }
                            // Material
                            if let Ok(m) = world.query_one_mut::<&mut MaterialUniform>(data.entity) {
                                if *m != data.material {
                                    self.undo_stack.push(EditorCommand::Material {
                                        entity: data.entity,
                                        old_material: *m,
                                    });
                                    self.redo_stack.clear();
                                }
                                *m = data.material;
                            }
                        }
                    }

                    // Handle pending menu actions
                    if self.pending_new_scene {
                        self.pending_new_scene = false;
                        if let LauncherState::Running { ref mut world, ref mut lighting } = self.state {
                            world.clear();
                            *lighting = SceneLoader::new_empty(world);
                            // Spawn a default cube so there's something to pick right away
                            if let Some(cpu_mesh) = self.mesh_registry.get("cube") {
                                let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
                                    &ctx.device, &cpu_mesh,
                                ));
                                let entity = world.spawn((
                                    aether_engine::ecs::components::Transform::default(),
                                    aether_engine::ecs::components::MeshHandle::new(gpu_mesh, "cube"),
                                    aether_engine::renderer::renderable::MaterialUniform {
                                        albedo: [0.8, 0.3, 0.2, 1.0],
                                        roughness: 0.5,
                                        metallic: 0.0,
                                        _pad: [0.0, 0.0],
                                    },
                                    aether_engine::ecs::components::Visibility::default(),
                                    aether_engine::ecs::components::Name("DefaultCube".into()),
                                ));
                                let _ = world.insert(entity, (aether_engine::ecs::components::Selected,));
                            }
                            self.camera = FlyCamera::default();
                            info!("New scene created");
                        }
                    }
                    if self.pending_open_dialog {
                        self.pending_open_dialog = false;
                        if let Some(path) = rfd::FileDialog::new().set_directory("./scenes").add_filter("RON", &["ron"]).pick_file() {
                            if let LauncherState::Running { ref mut world, ref mut lighting } = self.state {
                                match SceneLoader::open_scene(&path, &ctx.device, &self.mesh_registry, world) {
                                    Ok(new_lighting) => {
                                        *lighting = new_lighting;
                                        if let Some((pos, yaw, pitch, fov)) = read_camera_from_world(world) {
                                            self.camera.position = pos;
                                            self.camera.yaw = yaw;
                                            self.camera.pitch = pitch;
                                            self.camera.fov = fov;
                                            self.camera.active = false;
                                        }
                                        info!("Opened scene from {:?}", path);
                                    }
                                    Err(e) => {
                                        error!("Open scene error: {:?}", e);
                                    }
                                }
                            }
                        }
                    }
                    if self.pending_import_dialog {
                        self.pending_import_dialog = false;
                        if let Some(path) = rfd::FileDialog::new().set_directory("./scenes").add_filter("RON", &["ron"]).pick_file() {
                            if let LauncherState::Running { ref mut world, ref mut lighting } = self.state {
                                match SceneLoader::import_scene(&path, &ctx.device, &self.mesh_registry, world) {
                                    Ok(new_lighting) => {
                                        *lighting = new_lighting;
                                        info!("Imported scene from {:?}", path);
                                    }
                                    Err(e) => {
                                        error!("Import scene error: {:?}", e);
                                    }
                                }
                            }
                        }
                    }
                    if self.pending_save_dialog {
                        self.pending_save_dialog = false;
                        if let LauncherState::Running { ref mut world, ref lighting } = self.state {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory("./scenes")
                                .set_file_name("scene.ron")
                                .add_filter("RON scenes", &["ron"])
                                .save_file()
                            {
                                let path = if path.extension().map_or(true, |ext| ext != "ron") {
                                    path.with_extension("ron")
                                } else {
                                    path
                                };
                                write_camera_to_world(&self.camera, world);
                                let desc = aether_engine::scene::serializer::serialize_world(world, lighting, "Untitled");
                                info!("Saving scene: {} objects, {} lights, camera at {:?}", desc.objects.len(), desc.lights.len(), desc.camera.position);
                                match aether_engine::scene::serializer::to_ron_string(&desc) {
                                    Ok(ron) => {
                                        if let Err(e) = std::fs::write(&path, ron) {
                                            error!("Save scene error: {:?}", e);
                                        } else {
                                            info!("Saved scene to {:?}", path);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Serialize scene error: {:?}", e);
                                    }
                                }
                            }
                        }
                    }

                    // Handle add object actions
                    let mut added_entity: Option<Entity> = None;
                    if self.pending_add_cube {
                        self.pending_add_cube = false;
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            if let Some(cpu_mesh) = self.mesh_registry.get("cube") {
                                let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                                let entity = world.spawn((
                                    aether_engine::ecs::components::Transform::default(),
                                    aether_engine::ecs::components::MeshHandle::new(gpu_mesh, "cube"),
                                    aether_engine::renderer::renderable::MaterialUniform {
                                        albedo: [0.8, 0.3, 0.2, 1.0],
                                        roughness: 0.5,
                                        metallic: 0.0,
                                        _pad: [0.0, 0.0],
                                    },
                                    aether_engine::ecs::components::Visibility::default(),
                                    aether_engine::ecs::components::Name("Cube".into()),
                                ));
                                added_entity = Some(entity);
                                info!("Added cube entity {:?}", entity);
                            }
                        }
                    }
                    if self.pending_add_sphere {
                        self.pending_add_sphere = false;
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            if let Some(cpu_mesh) = self.mesh_registry.get("sphere") {
                                let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                                let entity = world.spawn((
                                    aether_engine::ecs::components::Transform::default(),
                                    aether_engine::ecs::components::MeshHandle::new(gpu_mesh, "sphere"),
                                    aether_engine::renderer::renderable::MaterialUniform {
                                        albedo: [0.2, 0.5, 0.8, 1.0],
                                        roughness: 0.05,
                                        metallic: 0.0,
                                        _pad: [0.0, 0.0],
                                    },
                                    aether_engine::ecs::components::Visibility::default(),
                                    aether_engine::ecs::components::Name("Sphere".into()),
                                ));
                                added_entity = Some(entity);
                                info!("Added sphere entity {:?}", entity);
                            }
                        }
                    }

                    // Auto-select newly added entity
                    if let Some(entity) = added_entity {
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            // Deselect all: collect first, then modify
                            let to_deselect: Vec<_> = world.query::<(Entity, &Selected)>().iter().map(|(e, _)| e).collect();
                            for e in to_deselect {
                                let _ = world.remove::<(aether_engine::ecs::components::Selected,)>(e);
                            }
                            // Select new entity
                            let _ = world.insert(entity, (aether_engine::ecs::components::Selected,));
                        }
                    }

                    // Handle hierarchy panel selection
                    if let Some(entity) = self.pending_select_entity.take() {
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            // Deselect all
                            let to_deselect: Vec<_> = world.query::<(Entity, &Selected)>().iter().map(|(e, _)| e).collect();
                            for e in to_deselect {
                                let _ = world.remove::<(aether_engine::ecs::components::Selected,)>(e);
                            }
                            // Select chosen entity
                            let _ = world.insert(entity, (aether_engine::ecs::components::Selected,));
                        }
                    }

                    // Handle despawn (delete entity)
                    if let Some(entity) = self.pending_despawn_entity.take() {
                        if let LauncherState::Running { ref mut world, .. } = self.state {
                            let _ = world.despawn(entity);
                        }
                    }

                    egui_winit_state
                        .handle_platform_output(window, egui_output.platform_output);
                    let pj = self.egui_ctx
                        .tessellate(egui_output.shapes, egui_output.pixels_per_point);
                    let sd = egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [ctx.config.width, ctx.config.height],
                        pixels_per_point: window.scale_factor() as f32,
                    };
                    (pj, egui_output.textures_delta, sd)
                };

                // Update egui textures before surface acquisition so that
                // the font atlas is allocated even if the surface is not
                // yet ready (Timeout/Occluded/Outdated on first frame).
                for (id, image_delta) in &textures_delta.set {
                    egui_renderer.update_texture(
                        &ctx.device, &ctx.queue, *id, image_delta,
                    );
                }

                // Acquire surface
                let output = match ctx.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(o) => o,
                    wgpu::CurrentSurfaceTexture::Suboptimal(o) => o,
                    wgpu::CurrentSurfaceTexture::Lost => {
                        ctx.resize(ctx.config.width, ctx.config.height);
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Validation => {
                        event_loop.exit();
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded
                    | wgpu::CurrentSurfaceTexture::Outdated => {
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

                match &mut self.state {
                    LauncherState::Menu => {
                        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Menu"),
                            color_attachments: &[Some(
                                wgpu::RenderPassColorAttachment {
                                    view: &target_view,
                                    depth_slice: None,
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
                            multiview_mask: None,
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                    }
                    LauncherState::Running {
                                ref world,
                                ref mut lighting,
                            } => {
                        let aspect =
                            ctx.config.width as f32 / ctx.config.height as f32;

                        // Update lighting from ECS (camera position + light data)
                        lighting.camera_pos = self.camera.position.to_array();
                        if let Some((transform, light)) = world.query::<(&aether_engine::ecs::components::Transform, &aether_engine::ecs::components::Light)>().iter().next() {
                            lighting.light.direction = (transform.rotation * glam::Vec3::NEG_Y).normalize().to_array();
                            lighting.light.color = light.color;
                            lighting.light.intensity = light.intensity;
                        }

                        // Extract phase: ECS World → GPU-ready batches
                        let batches = extract_render_batches(world);

                        // Transform gizmo: build dynamic debug lines for selected entity
                        let gizmo_lines = if let Some((_, transform)) = selected_entity_transform(world) {
                            build_transform_gizmo(&transform)
                        } else {
                            vec![]
                        };
                        scheduler.set_dynamic_lines(gizmo_lines);

                        // Build per-frame context — all passes extract
                        // what they need via apply_frame.
                        let frame = RenderFrame {
                            batches: &batches,
                            camera: &self.camera,
                            lighting,
                            queue: &ctx.queue,
                            aspect,
                            delta_time: dt,
                        };
                        scheduler.set_feature_flags(self.ssao_enabled, self.shadow_enabled, self.ibl_enabled);
                        scheduler.set_ssao_params(self.ssao_radius, self.ssao_bias, self.ssao_intensity);
                        scheduler.set_debug_mode(self.debug_mode as u32);
                        scheduler.set_ssr_debug_mode(self.ssr_debug_mode);
                        scheduler.set_ssr_enabled(self.ssr_enabled);
                        scheduler.apply_frame_all(&frame);
                        scheduler.execute_all(&mut encoder, &target_view);
                    }
                }

                // Screenshot copy before egui pass
                if should_screenshot {
                    let (size, bpr) =
                        screenshot_buffer_size(ctx.config.width, ctx.config.height);
                    self.screenshot_bytes_per_row = bpr;
                    let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Screenshot Buffer"),
                        size,
                        usage: wgpu::BufferUsages::COPY_DST
                            | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    });
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture: &output.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: &buf,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(bpr),
                                rows_per_image: Some(ctx.config.height),
                            },
                        },
                        wgpu::Extent3d {
                            width: ctx.config.width,
                            height: ctx.config.height,
                            depth_or_array_layers: 1,
                        },
                    );
                    self.screenshot_buffer = Some(buf);
                }

                // egui pass
                egui_renderer.update_buffers(
                    &ctx.device,
                    &ctx.queue,
                    &mut encoder,
                    &paint_jobs,
                    &screen_descriptor,
                );
                {
                    let rp = encoder.begin_render_pass(
                        &wgpu::RenderPassDescriptor {
                            label: Some("egui"),
                            color_attachments: &[Some(
                                wgpu::RenderPassColorAttachment {
                                    view: &target_view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                },
                            )],
                            multiview_mask: None,
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        },
                    );
                    egui_renderer.render(
                        &mut rp.forget_lifetime(),
                        &paint_jobs,
                        &screen_descriptor,
                    );
                }

                ctx.queue.submit(std::iter::once(encoder.finish()));
                output.present();
                self.input.end_frame();

                if should_screenshot {
                    if let Some(buf) = self.screenshot_buffer.take() {
                        if let Some(path) = self.pending_screenshot_path.take() {
                            if let Err(e) = save_screenshot(
                                &ctx.device,
                                &buf,
                                ctx.config.width,
                                ctx.config.height,
                                self.screenshot_bytes_per_row,
                                &path,
                            ) {
                                error!("Screenshot failed: {:?}", e);
                            } else {
                                info!("Screenshot saved to {:?}", path);
                            }
                            self.screenshot_taken = true;
                        }
                    }
                }

                if self.exit_after_frames.map_or(false, |n| self.frame_count >= n) {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    info!("Aether Engine Launcher starting...");

    let all_args: Vec<String> = std::env::args().collect();
    let cli = parse_args(&all_args);

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(cli);
    event_loop.run_app(&mut app).expect("Event loop error");
}
