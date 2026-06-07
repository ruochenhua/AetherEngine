//! Transform gizmo rendering and interaction.
//!
//! Provides screen-space hover detection and drag-to-translate along
//! world-space axes.

use crate::ecs::components::{Selected, Transform};
use crate::ecs::World;
use crate::renderer::passes::debug::DebugVertex;
use glam::{Vec2, Vec3, Vec4};

/// Which gizmo axis is being hovered or dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GizmoAxis {
    /// X axis (red).
    X,
    /// Y axis (green).
    Y,
    /// Z axis (blue).
    Z,
}

const GIZMO_LENGTH: f32 = 1.5;
const HOVER_PIXEL_THRESHOLD: f32 = 12.0;

/// Build debug-line vertices for a transform gizmo at the given entity position.
pub fn build_transform_gizmo(transform: &Transform) -> Vec<DebugVertex> {
    let origin = transform.translation;
    let l = GIZMO_LENGTH;

    vec![
        // X axis (red)
        DebugVertex {
            position: origin.to_array(),
            color: [1.0, 0.0, 0.0, 1.0],
        },
        DebugVertex {
            position: (origin + Vec3::X * l).to_array(),
            color: [1.0, 0.0, 0.0, 1.0],
        },
        // Y axis (green)
        DebugVertex {
            position: origin.to_array(),
            color: [0.0, 1.0, 0.0, 1.0],
        },
        DebugVertex {
            position: (origin + Vec3::Y * l).to_array(),
            color: [0.0, 1.0, 0.0, 1.0],
        },
        // Z axis (blue)
        DebugVertex {
            position: origin.to_array(),
            color: [0.0, 0.0, 1.0, 1.0],
        },
        DebugVertex {
            position: (origin + Vec3::Z * l).to_array(),
            color: [0.0, 0.0, 1.0, 1.0],
        },
    ]
}

/// Project a world-space point to screen-space pixels (top-left origin).
fn project_screen(
    world_pos: Vec3,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
) -> Vec2 {
    let clip = proj * view * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w.abs() < 1e-6 {
        return Vec2::new(-1.0, -1.0);
    }
    let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
    Vec2::new(
        (ndc.x * 0.5 + 0.5) * width,
        (1.0 - (ndc.y * 0.5 + 0.5)) * height,
    )
}

/// Squared distance from point `p` to line segment `a-b`.
fn point_segment_dist_sq(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.length_squared();
    if ab_len_sq < 1e-6 {
        return ap.length_squared();
    }
    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length_squared()
}

/// Detect which gizmo axis (if any) the mouse is hovering over.
pub fn detect_hover(
    transform: &Transform,
    view: glam::Mat4,
    proj: glam::Mat4,
    mouse_x: f32,
    mouse_y: f32,
    width: f32,
    height: f32,
) -> Option<GizmoAxis> {
    let origin = transform.translation;
    let l = GIZMO_LENGTH;
    let axes = [
        (GizmoAxis::X, origin + Vec3::X * l),
        (GizmoAxis::Y, origin + Vec3::Y * l),
        (GizmoAxis::Z, origin + Vec3::Z * l),
    ];

    let mouse = Vec2::new(mouse_x, mouse_y);
    let origin_screen = project_screen(origin, view, proj, width, height);

    let mut best: Option<(GizmoAxis, f32)> = None;
    for (axis, end_world) in &axes {
        let end_screen = project_screen(*end_world, view, proj, width, height);
        let dist_sq = point_segment_dist_sq(mouse, origin_screen, end_screen);
        let dist = dist_sq.sqrt();
        if dist < HOVER_PIXEL_THRESHOLD {
            if best.map_or(true, |(_, d)| dist < d) {
                best = Some((*axis, dist));
            }
        }
    }

    best.map(|(axis, _)| axis)
}

/// Apply drag translation along the given axis based on mouse screen delta.
///
/// `mouse_delta` is in screen pixels. The translation magnitude is scaled
/// by the camera distance so that dragging feels consistent at any zoom.
pub fn apply_drag(
    transform: &mut Transform,
    axis: GizmoAxis,
    mouse_delta: Vec2,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
    camera_pos: Vec3,
) {
    let axis_dir = match axis {
        GizmoAxis::X => Vec3::X,
        GizmoAxis::Y => Vec3::Y,
        GizmoAxis::Z => Vec3::Z,
    };

    // Project axis to screen to determine screen-space direction
    let origin_screen = project_screen(transform.translation, view, proj, width, height);
    let end_screen = project_screen(transform.translation + axis_dir * GIZMO_LENGTH, view, proj, width, height);
    let axis_screen = end_screen - origin_screen;

    if axis_screen.length_squared() < 1e-6 {
        return;
    }

    let axis_screen_dir = axis_screen.normalize();
    let screen_move = mouse_delta.dot(axis_screen_dir);

    // Scale screen pixels to world units based on camera distance
    let dist = (camera_pos - transform.translation).length().max(0.1);
    // Approx: 1 screen pixel ≈ dist * tan(fov/2) * 2 / height  world units
    // We use a simpler empirical scale
    let world_scale = dist * 0.002;
    let world_move = screen_move * world_scale;

    match axis {
        GizmoAxis::X => transform.translation.x += world_move,
        GizmoAxis::Y => transform.translation.y += world_move,
        GizmoAxis::Z => transform.translation.z += world_move,
    }
}

/// Query the world for the selected entity and its transform.
pub fn selected_entity_transform(world: &World) -> Option<(hecs::Entity, Transform)> {
    for (entity, transform, _) in world.query::<(hecs::Entity, &Transform, &Selected)>().iter() {
        return Some((entity, transform.clone()));
    }
    None
}
