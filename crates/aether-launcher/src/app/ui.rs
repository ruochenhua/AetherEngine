//! egui UI rendering for the launcher.

mod animation_panel;
mod asset_drop;
mod editor_operations;
mod hierarchy;
mod physics_panel;
mod renderer_controls;

use super::{App, LauncherState};
use crate::inspector::{self, InspectorTarget};
use aether_engine::ecs::components::Camera;
use aether_engine::ecs::Entity;
use tracing::error;

/// Render the full launcher UI and return egui draw data.
pub(crate) fn render(
    app: &mut App,
    dt: f32,
) -> (
    Vec<egui::ClippedPrimitive>,
    egui::TexturesDelta,
    egui_wgpu::ScreenDescriptor,
) {
    app.logs.drain();
    let window = app.window.as_ref().unwrap().clone();
    let raw_input = app
        .egui_winit_state
        .as_mut()
        .unwrap()
        .take_egui_input(&window);
    let skip_egui = app.no_gui_overlay && matches!(app.state, LauncherState::Running { .. });

    if skip_egui {
        let ctx = app.ctx.as_ref().unwrap();
        let sd = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [ctx.config.width, ctx.config.height],
            pixels_per_point: window.scale_factor() as f32,
        };
        return (vec![], egui::TexturesDelta::default(), sd);
    }

    let egui_output = draw(app, dt, raw_input);

    let egui_winit_state = app.egui_winit_state.as_mut().unwrap();
    egui_winit_state.handle_platform_output(&window, egui_output.platform_output);
    let pj = app
        .egui_ctx
        .tessellate(egui_output.shapes, egui_output.pixels_per_point);
    let sd = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [
            app.ctx.as_ref().unwrap().config.width,
            app.ctx.as_ref().unwrap().config.height,
        ],
        pixels_per_point: window.scale_factor() as f32,
    };
    (pj, egui_output.textures_delta, sd)
}

/// Draw editor UI without a GPU/window so input behavior can be exercised directly.
pub(crate) fn draw(app: &mut App, dt: f32, raw_input: egui::RawInput) -> egui::FullOutput {
    app.logs.drain();
    if app.no_gui_overlay && matches!(app.state, LauncherState::Running { .. }) {
        return egui::FullOutput::default();
    }
    // Extract mutable references needed by the egui closure
    let scroll_input = &mut app.scroll_input;
    let state = &app.state;
    let scene_entries = &app.scene_entries;
    let pending_load = &mut app.pending_load;
    let camera = &app.camera;
    let fps = app.fps;
    let gpu_info = &app.gpu_info;
    let gpu_timer_supported = app.gpu_timer_supported;
    let mut render_options = renderer_controls::RenderOptions {
        ssao_enabled: &mut app.ssao_enabled,
        ssao_radius: &mut app.ssao_radius,
        ssao_bias: &mut app.ssao_bias,
        ssao_intensity: &mut app.ssao_intensity,
        shadow_enabled: &mut app.shadow_enabled,
        ibl_enabled: &mut app.ibl_enabled,
        ssr_enabled: &mut app.ssr_enabled,
        tone_mapping_mode: &mut app.tone_mapping_mode,
        bloom_enabled: &mut app.bloom_enabled,
        bloom_threshold: &mut app.bloom_threshold,
        bloom_intensity: &mut app.bloom_intensity,
        fxaa_enabled: &mut app.fxaa_enabled,
        fxaa_quality: &mut app.fxaa_quality,
        fxaa_edge_threshold: &mut app.fxaa_edge_threshold,
    };
    let ssr_debug_mode = app.ssr_debug_mode;
    let show_overlay = &mut app.show_overlay;
    let fullscreen_3d = &mut app.fullscreen_3d;
    let workspace = &mut app.workspace;
    let logs = &mut app.logs;
    let pending_asset_import = app
        .pending_asset_import
        .as_ref()
        .map(|pending| (pending.destination.as_str(), pending.source.as_path()));
    let mut project_open_request = None;
    let mut project_file_request = None;
    let mut confirm_asset_import = false;
    let mut cancel_asset_import = false;
    let mut viewport_drop_request = None;
    let mut material_drop_request = None;
    let debug_mode = app.debug_mode;
    let pending_select_entity = &mut app.pending_select_entity;
    let pending_editor_operation = &mut app.pending_editor_operation;
    let pending_asset_drop = &mut app.pending_asset_drop;
    let has_saved_scene = app.cli.scene.is_some();
    let editor_operation_message = app.editor_operation_message.clone();
    let playback_state = app.particle_runtime.playback;
    let playback_action = &mut app.particle_runtime.pending_action;
    let physics_debug_enabled = &mut app.cli.physics_debug_enabled;
    let physics_body_count = app
        .particle_runtime
        .physics
        .as_ref()
        .map_or(0, aether_engine::physics::PhysicsRuntime::entity_count);
    let physics_collider_count = app
        .particle_runtime
        .physics
        .as_ref()
        .map_or(0, aether_engine::physics::PhysicsRuntime::collider_count);
    let mut animation_controls =
        app.animation_fixture
            .as_ref()
            .map(|fixture| animation_panel::Controls {
                clip_name: fixture.clip_name().to_owned(),
                time: fixture.time(),
                duration: fixture.duration(),
                playing: fixture.is_playing(),
                speed: fixture.speed(),
                looping: fixture.is_looping(),
            });
    let mut animation_changes = animation_panel::Changes::default();

    // Extract inspector data before UI (mutable borrow needed for editing)
    let mut inspector_target: Option<InspectorTarget> = None;
    let mut selected_entity: Option<Entity> = None;
    if let LauncherState::Running { ref world, .. } = app.state {
        if let Some(target) = inspector::extract(world) {
            selected_entity = Some(target.entity());
            inspector_target = Some(target);
        }
    }

    let hierarchy_items = match &app.state {
        LauncherState::Running { world, .. } => hierarchy::collect(world),
        LauncherState::Menu => Vec::new(),
    };
    let selected_operation_name = selected_entity.and_then(|entity| {
        hierarchy_items
            .iter()
            .find(|item| item.entity == entity)
            .and_then(|item| item.operation_name.clone())
    });

    let egui_output = app.egui_ctx.run_ui(raw_input, |ui| {
        *scroll_input += ui.ctx().input(|i| i.smooth_scroll_delta.y * 0.01);

        if (matches!(state, LauncherState::Menu) || *show_overlay) && !*fullscreen_3d {
            let requests = workspace.render(
                ui,
                editor_operation_message.as_deref(),
                logs,
                pending_asset_import,
            );
            project_open_request = requests.open_scene;
            project_file_request = requests.project_file;
            confirm_asset_import = requests.confirm_import;
            cancel_asset_import = requests.cancel_import;
        }

        match state {
            LauncherState::Menu => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.heading("Aether Engine Scenes");
                    ui.separator();
                    if scene_entries.is_empty() {
                        ui.label("No scenes found.");
                    } else {
                        for (idx, entry) in scene_entries.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}. {}", idx + 1, entry.name))
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
                    egui::Panel::top("menu_bar").show_inside(ui, |ui| {
                        egui::MenuBar::new().ui(ui, |ui| {
                            ui.menu_button("File", |ui| {
                                if ui.button("New Scene").clicked() {
                                    app.pending_new_scene = true;
                                    ui.close();
                                }
                                if ui.button("Open Scene...").clicked() {
                                    app.pending_open_dialog = true;
                                    ui.close();
                                }
                                if ui.button("Import Scene...").clicked() {
                                    app.pending_import_dialog = true;
                                    ui.close();
                                }
                                if ui.button("Save Scene...").clicked() {
                                    app.pending_save_dialog = true;
                                    ui.close();
                                }
                            });
                            ui.menu_button("Create", |ui| {
                                let (response, operation) = editor_operations::add_light_button(
                                    ui,
                                    selected_operation_name.as_deref(),
                                    has_saved_scene,
                                );
                                if response.clicked() {
                                    *pending_editor_operation = operation;
                                }
                                let (response, operation) =
                                    editor_operations::add_spot_light_button(
                                        ui,
                                        selected_operation_name.as_deref(),
                                        has_saved_scene,
                                    );
                                if response.clicked() {
                                    *pending_editor_operation = operation;
                                }
                                if let Some(message) = editor_operation_message.as_deref() {
                                    ui.small(message);
                                }
                                ui.separator();
                                if ui.button("Add Cube").clicked() {
                                    app.pending_add_cube = true;
                                    ui.close();
                                }
                                if ui.button("Add Sphere").clicked() {
                                    app.pending_add_sphere = true;
                                    ui.close();
                                }
                                if ui.button("Add Terrain").clicked() {
                                    app.pending_add_terrain = true;
                                    ui.close();
                                }
                                if ui.button("Add Water").clicked() {
                                    app.pending_add_water = true;
                                    ui.close();
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = if *fullscreen_3d {
                                        "⛶ Exit Full Screen"
                                    } else {
                                        "⛶ Full Screen"
                                    };
                                    if ui.button(label).clicked() {
                                        *fullscreen_3d = !*fullscreen_3d;
                                    }
                                },
                            );
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
                                                app.pending_despawn_entity = Some(item.entity);
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
                                    physics_panel::render(
                                        ui,
                                        playback_state,
                                        playback_action,
                                        physics_debug_enabled,
                                        physics_body_count,
                                        physics_collider_count,
                                    );

                                    if let Some(controls) = animation_controls.as_mut() {
                                        animation_changes = animation_panel::render(ui, controls);
                                    }

                                    // Inspector
                                    if let Some(ref mut target) = inspector_target.as_mut() {
                                        material_drop_request = inspector::render(ui, target);
                                    }

                                    egui::CollapsingHeader::new("Scene Info")
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            ui.label(format!("GPU: {}", gpu_info));
                                            ui.label(format!(
                                                "GPU Timer: {}",
                                                if gpu_timer_supported {
                                                    "supported"
                                                } else {
                                                    "unsupported"
                                                }
                                            ));
                                            ui.label(format!("Scenes: {}", scene_entries.len()));
                                            ui.separator();
                                            ui.label(format!("FPS: {:.1}", fps));
                                            ui.label(format!("Frame: {:.2} ms", dt * 1000.0));
                                            ui.label(format!("Entities: {}", world.len()));
                                            let p = camera.position;
                                            ui.label(format!(
                                                "Camera: ({:.1}, {:.1}, {:.1})",
                                                p.x, p.y, p.z
                                            ));
                                            ui.label(format!("Speed: {:.1}", camera.speed));
                                            let mode_names = [
                                                "Full",
                                                "Ambient",
                                                "Diffuse",
                                                "Specular",
                                                "Normals",
                                                "NdotL",
                                                "Shadow",
                                                "Direct",
                                                "IBL",
                                                "Alpha(P)",
                                                "Alpha(N)",
                                                "NDC(F2)",
                                                "EnvFix(F3)",
                                                "VDir(F4)",
                                                "SSAO(F5)",
                                                "CSM(F7)",
                                            ];
                                            let mode_idx = debug_mode.clamp(0, 15) as usize;
                                            ui.label(format!(
                                                "Debug: [{}] {}",
                                                mode_idx, mode_names[mode_idx]
                                            ));
                                            ui.label(format!("SSR Debug: {} (F6)", ssr_debug_mode));
                                        });

                                    egui::CollapsingHeader::new("Render Features")
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            renderer_controls::render(ui, &mut render_options);
                                        });

                                    egui::CollapsingHeader::new("Input Debug")
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            let (mdx, mdy) = app.input.mouse_delta();
                                            ui.label(format!(
                                                "Mouse delta: ({:.1}, {:.1})",
                                                mdx, mdy
                                            ));
                                            ui.label(format!("Alt held: {}", app.input.alt_held()));
                                            ui.label(format!(
                                                "Left held: {}",
                                                app.input
                                                    .mouse_held(winit::event::MouseButton::Left)
                                            ));
                                        });
                                });
                            });
                    }
                }

                // Central panel: 3D viewport (transparent so 3D render shows through)
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| {
                        viewport_drop_request = asset_drop::capture_viewport_drop(ui);
                    });
            }
        }
    });

    if project_open_request.is_some() {
        app.pending_project_open = project_open_request;
    }
    if project_file_request.is_some() {
        app.pending_project_file = project_file_request;
    }
    if confirm_asset_import {
        app.pending_asset_import_confirm = true;
    }
    if cancel_asset_import {
        app.pending_asset_import = None;
        app.pending_asset_import_confirm = false;
    }
    *pending_asset_drop = viewport_drop_request;
    app.pending_material_drop = material_drop_request;

    // Write back inspector changes
    if app.editor_write_degraded.is_none() {
        if let Some(ref target) = inspector_target {
            let undo_count = app.undo_stack.len();
            if let LauncherState::Running { ref mut world, .. } = app.state {
                let _ = inspector::apply(
                    target,
                    world,
                    &mut app.undo_stack,
                    &mut app.redo_stack,
                    &mut app.asset_manager,
                );
                if let inspector::InspectorTarget::ParticleEmitter {
                    entity, restart, ..
                } = target
                {
                    if let Err(message) = app.particle_runtime.configure(world) {
                        error!("Particle emitter edit rejected: {message}");
                    } else if *restart {
                        let entity_bits = entity.to_bits().get();
                        if let Err(message) = super::particle_runtime::restart_emitter(
                            &mut app.particle_runtime.particles,
                            &mut app.cli.time,
                            entity_bits,
                        ) {
                            error!("Particle emitter restart failed: {message}");
                        }
                    }
                }
                // If the camera was edited, sync intrinsic params to the runtime fly camera.
                if matches!(target, inspector::InspectorTarget::Camera { .. }) {
                    if let Some(camera) = world.query::<&Camera>().iter().next() {
                        app.camera.fov = camera.fov;
                        app.camera.speed = camera.speed;
                        app.camera.base_speed = camera.speed;
                        app.camera.near = camera.near;
                        app.camera.far = camera.far;
                    }
                }
            }
            if app.undo_stack.len() > undo_count {
                app.undo_action_order
                    .push(super::editor::UndoActionKind::Legacy);
                app.redo_action_order.clear();
            }
        }
    }

    if let (Some(fixture), Some(controls)) =
        (app.animation_fixture.as_mut(), animation_controls.as_ref())
    {
        if let Err(message) = fixture.apply_controls(
            controls.speed,
            controls.looping,
            animation_changes.command,
            animation_changes.seek,
        ) {
            error!("Animation control edit rejected: {message}");
        }
    }

    egui_output
}
