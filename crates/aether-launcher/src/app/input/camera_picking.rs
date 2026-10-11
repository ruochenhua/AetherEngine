//! Fly camera, editor picking, and transform gizmo input.

use super::super::{App, LauncherState};
use super::physics_picking;
use aether_engine::{
    renderer::gizmo::{apply_drag, detect_hover, selected_entity_transform, GizmoCameraCtx},
    renderer::picking::screen_ray,
};
use glam::Vec2;
use winit::event::MouseButton;

fn pick_at_screen_position(
    world: &mut aether_engine::ecs::World,
    physics: Option<&aether_engine::physics::PhysicsRuntime>,
    mouse_position: Vec2,
    viewport_size: Vec2,
    camera: &aether_engine::renderer::camera::FlyCamera,
) {
    let view = camera.view_matrix();
    let proj = camera.projection_matrix(viewport_size.x / viewport_size.y);
    let ray = screen_ray(
        mouse_position.x,
        mouse_position.y,
        viewport_size.x,
        viewport_size.y,
        view,
        proj,
        camera.position,
    );
    physics_picking::pick_scene_entity(world, physics, &ray);
}

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

    let mut legacy_edit_recorded = false;
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
                                    legacy_edit_recorded = true;
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
                        pick_at_screen_position(
                            world,
                            app.particle_runtime.physics.as_ref(),
                            Vec2::new(mx, my),
                            Vec2::new(width, height),
                            &app.camera,
                        );
                    }
                } else {
                    pick_at_screen_position(
                        world,
                        app.particle_runtime.physics.as_ref(),
                        Vec2::new(mx, my),
                        Vec2::new(width, height),
                        &app.camera,
                    );
                }
            }
        }
    }
    if legacy_edit_recorded {
        super::super::editor::mark_legacy_edit(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{
        asset::mesh::{CpuMesh, GpuMesh},
        ecs::{
            components::{MeshHandle, MeshSource, Selected, Transform},
            Entity, World,
        },
        physics::{Collider, ColliderShape, PhysicsRuntime, RigidBody},
        renderer::camera::FlyCamera,
    };
    use glam::{Vec2, Vec3};
    use std::sync::Arc;

    #[test]
    fn screen_pixel_ray_selects_the_physics_entity_under_the_cursor() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("screen picking test needs a headless GPU adapter");
        let device = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("screen picking test needs a headless GPU device")
            .0;
        let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
        let mut world = World::new();
        let target = world.spawn((
            Transform {
                translation: Vec3::new(0.7, 0.0, 0.0),
                ..Transform::default()
            },
            MeshHandle::new(mesh, MeshSource::Builtin("cube".into()), "cursor target"),
        ));
        world.spawn((Selected,));
        let mut physics = PhysicsRuntime::default();
        physics
            .spawn_body(
                target.to_bits().get(),
                &Transform {
                    translation: Vec3::new(0.7, 0.0, 0.0),
                    ..Transform::default()
                },
                &RigidBody::default(),
                &Collider {
                    shape: ColliderShape::Sphere(0.4),
                    ..Collider::default()
                },
            )
            .unwrap();
        let camera = FlyCamera {
            position: Vec3::new(0.0, 0.0, 5.0),
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            fov: std::f32::consts::FRAC_PI_4,
            ..FlyCamera::default()
        };

        pick_at_screen_position(
            &mut world,
            Some(&physics),
            Vec2::new(500.0, 300.0),
            Vec2::new(800.0, 600.0),
            &camera,
        );

        let selected = world
            .query::<(Entity, &Selected)>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        assert_eq!(selected, vec![target]);
    }
}
