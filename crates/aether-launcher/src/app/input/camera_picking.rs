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
    let mut viewport_camera_changed = false;
    if !egui_consumed
        && !app.egui_ctx.egui_wants_keyboard_input()
        && matches!(app.state, LauncherState::Running { .. })
    {
        let previous_camera = app.camera.clone();
        let (dx, dy) = app.input.mouse_delta();
        let zoom_delta = app.scroll_input + app.input.magnification_delta();
        app.camera.update(dt, dx, dy, zoom_delta, &app.input);
        app.scroll_input = 0.0;
        if app.input.alt_held() && app.input.mouse_held(MouseButton::Left) {
            if let LauncherState::Running { world, .. } = &app.state {
                if let Some((_, transform)) = selected_entity_transform(world) {
                    orbit_camera_around(&mut app.camera, transform.translation, dx, dy);
                }
            }
        }
        if app.input.key_pressed(winit::keyboard::KeyCode::KeyF)
            && !egui_consumed
            && !app.egui_ctx.egui_wants_keyboard_input()
        {
            if let LauncherState::Running { world, .. } = &app.state {
                if let Some((_, transform)) = selected_entity_transform(world) {
                    let scale = transform.scale.abs().max_element().max(1.0);
                    frame_camera_on(&mut app.camera, transform.translation, scale * 3.5);
                }
            }
        }
        viewport_camera_changed = camera_document_state_changed(&previous_camera, &app.camera);
    }
    app.scroll_input = 0.0;

    super::process_scene_shortcuts(app, app.input.key_pressed(winit::keyboard::KeyCode::Delete));

    let mut legacy_edit_recorded = false;
    if !egui_consumed
        && app.editor_write_degraded.is_none()
        && app.particle_runtime.playback
            == super::super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
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
    } else if viewport_camera_changed {
        super::super::editor::refresh_document_state(app);
    }
}

fn orbit_camera_around(
    camera: &mut aether_engine::renderer::camera::FlyCamera,
    target: glam::Vec3,
    mouse_dx: f32,
    mouse_dy: f32,
) {
    let radius = camera.position.distance(target);
    if !radius.is_finite() || radius <= f32::EPSILON {
        return;
    }
    camera.yaw -= mouse_dx * camera.sensitivity;
    camera.pitch = (camera.pitch + mouse_dy * camera.sensitivity).clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );
    camera.position = target - camera.forward() * radius;
}

fn frame_camera_on(
    camera: &mut aether_engine::renderer::camera::FlyCamera,
    target: glam::Vec3,
    distance: f32,
) {
    let direction = (target - camera.position).normalize_or_zero();
    if direction == glam::Vec3::ZERO {
        camera.position = target - camera.forward() * distance;
        return;
    }
    camera.yaw = direction.z.atan2(direction.x);
    camera.pitch = direction.y.clamp(-1.0, 1.0).asin();
    camera.position = target - camera.forward() * distance.max(0.5);
}

fn camera_document_state_changed(
    previous: &aether_engine::renderer::camera::FlyCamera,
    current: &aether_engine::renderer::camera::FlyCamera,
) -> bool {
    previous.position != current.position
        || previous.yaw != current.yaw
        || previous.pitch != current.pitch
        || previous.speed != current.speed
        || previous.fov != current.fov
        || previous.near != current.near
        || previous.far != current.far
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
    use winit::event::{DeviceId, ElementState, WindowEvent};

    fn app_with_selected_transform(transform: Transform) -> App {
        let mut app =
            App::new(super::super::super::cli::parse_args(&["aether-launcher".into()]).unwrap());
        let mut world = World::new();
        world.spawn((transform, Selected));
        app.state = LauncherState::Running {
            world,
            lighting: Default::default(),
        };
        app.particle_runtime.playback =
            super::super::super::particle_runtime::physics_runtime::PlaybackState::Playing;
        app
    }

    fn update_navigation(app: &mut App) {
        update_camera_and_picking(app, 1.0 / 60.0, false);
    }

    #[test]
    fn framing_target_aims_camera_and_uses_requested_distance() {
        let target = Vec3::new(1.0, 2.0, -3.0);
        let mut camera = FlyCamera::default();

        frame_camera_on(&mut camera, target, 4.0);

        let toward_target = (target - camera.position).normalize();
        assert!(camera.forward().dot(toward_target) > 0.999);
        assert!((camera.position.distance(target) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn option_left_drag_orbits_around_the_selected_transform() {
        let mut app = app_with_selected_transform(Transform::default());
        app.camera.position = Vec3::new(0.0, 0.0, 10.0);
        app.camera.yaw = -std::f32::consts::FRAC_PI_2;
        app.camera.pitch = 0.0;
        app.input
            .handle_window_event(&WindowEvent::ModifiersChanged(
                winit::keyboard::ModifiersState::ALT.into(),
            ));
        app.input.handle_window_event(&WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        });
        app.input.handle_window_event(&WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(20.0, 0.0),
        });

        update_navigation(&mut app);

        assert_ne!(app.camera.position, Vec3::new(0.0, 0.0, 10.0));
        assert!((app.camera.position.length() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn mac_trackpad_pinch_changes_view_distance() {
        let mut app = app_with_selected_transform(Transform::default());
        let start = app.camera.position;
        app.input.handle_window_event(&WindowEvent::PinchGesture {
            device_id: DeviceId::dummy(),
            delta: 0.5,
            phase: winit::event::TouchPhase::Moved,
        });

        update_navigation(&mut app);

        assert_ne!(app.camera.position, start);
    }

    #[test]
    fn scroll_consumed_by_egui_does_not_leak_into_the_next_viewport_frame() {
        let mut app = app_with_selected_transform(Transform::default());
        app.scroll_input = 1.0;
        let start = app.camera.position;

        update_camera_and_picking(&mut app, 1.0 / 60.0, true);

        assert_eq!(app.camera.position, start);
        assert_eq!(app.scroll_input, 0.0);
    }

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
