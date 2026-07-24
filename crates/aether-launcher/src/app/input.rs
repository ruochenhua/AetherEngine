//! Per-frame input handling: debug hotkeys, camera movement, picking and gizmos.

use super::{App, LauncherState};
use aether_engine::renderer::gizmo::{apply_drag, detect_hover, entity_transform, GizmoCameraCtx};
use aether_engine::renderer::picking::{pick_entity, screen_ray};
use winit::{event::MouseButton, keyboard::KeyCode};

/// Process number / function-key debug overlays.
pub(crate) fn process_debug_hotkeys(app: &mut App) {
    // Only process debug hotkeys when egui is not capturing keyboard input
    // (e.g. when typing in an Inspector text field).
    if app.egui_ctx.egui_wants_keyboard_input() {
        return;
    }

    if app.input.key_pressed(KeyCode::Digit0) {
        app.debug_mode = 0;
    }
    if app.input.key_pressed(KeyCode::Digit1) {
        app.debug_mode = 1;
    }
    if app.input.key_pressed(KeyCode::Digit2) {
        app.debug_mode = 2;
    }
    if app.input.key_pressed(KeyCode::Digit3) {
        app.debug_mode = 3;
    }
    if app.input.key_pressed(KeyCode::Digit4) {
        app.debug_mode = 4;
    }
    if app.input.key_pressed(KeyCode::Digit5) {
        app.debug_mode = 5;
    }
    if app.input.key_pressed(KeyCode::Digit6) {
        app.debug_mode = 6;
    }
    if app.input.key_pressed(KeyCode::Digit7) {
        app.debug_mode = 7;
    }
    if app.input.key_pressed(KeyCode::Digit8) {
        app.debug_mode = 8;
    }
    if app.input.key_pressed(KeyCode::Digit9) {
        app.debug_mode = 9;
    }
    if app.input.key_pressed(KeyCode::F1) {
        app.debug_mode = 10;
    }
    if app.input.key_pressed(KeyCode::F2) {
        app.debug_mode = 11;
    }
    if app.input.key_pressed(KeyCode::F3) {
        app.debug_mode = 12;
    }
    if app.input.key_pressed(KeyCode::F4) {
        app.debug_mode = 13;
    }
    if app.input.key_pressed(KeyCode::F5) {
        app.debug_mode = 14;
    }
    if app.input.key_pressed(KeyCode::F6) {
        app.ssr_debug_mode = (app.ssr_debug_mode + 1) % 10;
    }
    if app.input.key_pressed(KeyCode::F7) {
        app.debug_mode = 15;
    }
}

/// Update the fly camera, picking and gizmo interaction for the current frame.
pub(crate) fn update_camera_and_picking(app: &mut App, dt: f32, egui_consumed: bool) {
    // Reconcile the ordered selection with the ECS `Selected` markers; they
    // may have changed via undo/redo, scene load, or despawns last frame.
    if let LauncherState::Running { ref world, .. } = app.state {
        app.selection.sync(world);
    }

    // Camera update (only when pointer is not over egui UI)
    if !egui_consumed && matches!(app.state, LauncherState::Running { .. }) {
        let (dx, dy) = app.input.mouse_delta();
        app.camera.update(dt, dx, dy, app.scroll_input, &app.input);
        app.scroll_input = 0.0;
    }

    // Delete all selected entities on Delete key. Suppressed while egui
    // captures keyboard input (e.g. editing an Inspector text field), same
    // guard as the debug hotkeys above.
    if app.input.key_pressed(KeyCode::Delete)
        && !app.selection.is_empty()
        && !app.egui_ctx.egui_wants_keyboard_input()
    {
        app.pending_delete_selection = true;
    }

    // Picking + Gizmo interaction (only in Running state, and not over UI)
    if !egui_consumed {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let ctx = app.ctx.as_ref().unwrap();
            let width = ctx.config.width as f32;
            let height = ctx.config.height as f32;
            let (mx, my) = app.input.mouse_position();
            let view = app.camera.view_matrix();
            let proj = app.camera.projection_matrix(width / height);
            let mouse_pressed = app.input.mouse_pressed(MouseButton::Left) && !app.input.alt_held();
            let mouse_held = app.input.mouse_held(MouseButton::Left) && !app.input.alt_held();
            let mouse_released = app.input.mouse_released(MouseButton::Left);
            // Shift/Ctrl + click toggles selection instead of single-selecting.
            let additive = app.input.shift_held() || app.input.ctrl_held();
            // The gizmo anchors to the most recently selected entity.
            let anchor = app.selection.anchor();
            let anchor_transform = anchor.and_then(|entity| entity_transform(world, entity));

            // Gizmo drag handling
            if let Some(axis) = app.gizmo_drag_axis {
                if mouse_held {
                    let (dx, dy) = app.input.mouse_delta();
                    if let Some(entity) = anchor {
                        if let Ok(transform) = world
                            .query_one_mut::<&mut aether_engine::ecs::components::Transform>(entity)
                        {
                            apply_drag(
                                transform,
                                axis,
                                glam::Vec2::new(dx, dy),
                                &GizmoCameraCtx {
                                    view,
                                    proj,
                                    width,
                                    height,
                                    camera_pos: app.camera.position,
                                },
                            );
                        }
                    }
                }
                if mouse_released {
                    // Record undo command if transform changed during drag
                    if let Some(entity) = anchor {
                        if let Some(old_transform) = app.gizmo_drag_start_transform.take() {
                            if let Ok(transform) = world
                                .query_one_mut::<&mut aether_engine::ecs::components::Transform>(
                                entity,
                            ) {
                                if *transform != old_transform {
                                    app.undo_stack.push(
                                        crate::inspector::EditorCommand::Transform {
                                            entity,
                                            old_transform,
                                        },
                                    );
                                    app.redo_stack.clear();
                                }
                            }
                        }
                    }
                    app.gizmo_drag_axis = None;
                }
            } else if mouse_pressed {
                // Check gizmo hover before picking
                if let Some(transform) = anchor_transform {
                    if let Some(hovered) =
                        detect_hover(&transform, view, proj, mx, my, width, height)
                    {
                        app.gizmo_drag_axis = Some(hovered);
                        app.gizmo_drag_start_transform = Some(transform.clone());
                    } else {
                        // Not hovering gizmo: perform picking
                        let ray =
                            screen_ray(mx, my, width, height, view, proj, app.camera.position);
                        pick_entity(world, &ray, additive);
                        app.selection.sync(world);
                    }
                } else {
                    // No selection: perform picking
                    let ray = screen_ray(mx, my, width, height, view, proj, app.camera.position);
                    pick_entity(world, &ray, additive);
                    app.selection.sync(world);
                }
            }
        }
    }
}
