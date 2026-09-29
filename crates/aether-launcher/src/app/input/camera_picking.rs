//! Fly camera, editor picking, and transform gizmo input.

use super::super::{App, LauncherState};
use super::physics_picking;
use aether_engine::{
    renderer::gizmo::{apply_drag, detect_hover, selected_entity_transform, GizmoCameraCtx},
    renderer::picking::screen_ray,
};
use winit::event::MouseButton;

/// Update camera, picking, and gizmo interaction for the current frame.
pub(crate) fn update_camera_and_picking(app: &mut App, dt: f32, egui_consumed: bool) {
    if !egui_consumed && matches!(app.state, LauncherState::Running { .. }) {
        let (dx, dy) = app.input.mouse_delta();
        app.camera.update(dt, dx, dy, app.scroll_input, &app.input);
        app.scroll_input = 0.0;
    }

    if app.input.key_pressed(winit::keyboard::KeyCode::Delete) {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            if let Some((entity, _)) = selected_entity_transform(world) {
                app.pending_despawn_entity = Some(entity);
            }
        }
    }

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

            if let Some(axis) = app.gizmo_drag_axis {
                if mouse_held {
                    let (dx, dy) = app.input.mouse_delta();
                    if let Some((entity, _)) = selected_entity_transform(world) {
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
                    if let Some((entity, _)) = selected_entity_transform(world) {
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
                if let Some((_, transform)) = selected_entity_transform(world) {
                    if let Some(hovered) =
                        detect_hover(&transform, view, proj, mx, my, width, height)
                    {
                        app.gizmo_drag_axis = Some(hovered);
                        app.gizmo_drag_start_transform = Some(transform.clone());
                    } else {
                        let ray =
                            screen_ray(mx, my, width, height, view, proj, app.camera.position);
                        physics_picking::pick_scene_entity(
                            world,
                            app.particle_runtime.physics.as_ref(),
                            &ray,
                        );
                    }
                } else {
                    let ray = screen_ray(mx, my, width, height, view, proj, app.camera.position);
                    physics_picking::pick_scene_entity(
                        world,
                        app.particle_runtime.physics.as_ref(),
                        &ray,
                    );
                }
            }
        }
    }
}
