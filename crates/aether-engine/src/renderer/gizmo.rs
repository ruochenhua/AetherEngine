//! Transform gizmo rendering and interaction.
//!
//! Provides screen-space hover detection and drag-to-manipulate for
//! translate, rotate, and scale along world-space axes.

use crate::ecs::components::{Selected, Transform};
use crate::ecs::World;
use crate::renderer::passes::debug::DebugVertex;
use glam::{Quat, Vec2, Vec3, Vec4};

/// Which axis (X/Y/Z) the gizmo handle targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GizmoAxis {
    /// World-space X axis (red).
    X,
    /// World-space Y axis (green).
    Y,
    /// World-space Z axis (blue).
    Z,
}

/// What kind of gizmo handle and which axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GizmoHandle {
    /// Translation arrow along the given axis.
    Translate(GizmoAxis),
    /// Rotation ring around the given axis.
    Rotate(GizmoAxis),
    /// Scale box along the given axis.
    Scale(GizmoAxis),
}

// Lengths for different gizmo components
const TRANS_LEN: f32 = 1.5;
const SCALE_LEN: f32 = 1.2;
const ROT_RADIUS: f32 = 1.5;
const ROT_SEGMENTS: usize = 64;

const HOVER_PIXEL_THRESHOLD: f32 = 10.0;

/// Build debug-line vertices for a transform gizmo at the given entity position.
pub fn build_transform_gizmo(transform: &Transform) -> Vec<DebugVertex> {
    let origin = transform.translation;
    let mut lines = Vec::new();

    // --- Translate axes (bright) ---
    let t_axes = [
        (Vec3::X, [1.0, 0.2, 0.2, 1.0]),
        (Vec3::Y, [0.2, 1.0, 0.2, 1.0]),
        (Vec3::Z, [0.2, 0.2, 1.0, 1.0]),
    ];
    for (dir, color) in &t_axes {
        let end = origin + *dir * TRANS_LEN;
        lines.push(DebugVertex { position: origin.to_array(), color: *color });
        lines.push(DebugVertex { position: end.to_array(), color: *color });
        // Arrow head (small pyramid hint: two short perpendicular lines)
        let head_size = 0.08;
        let perp1 = if dir.dot(Vec3::Y).abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        let perp2 = dir.cross(perp1).normalize();
        let perp1 = perp1.cross(*dir).normalize();
        let tip = end;
        let base1 = tip - *dir * head_size * 2.0 + perp1 * head_size;
        let base2 = tip - *dir * head_size * 2.0 - perp1 * head_size;
        let base3 = tip - *dir * head_size * 2.0 + perp2 * head_size;
        let base4 = tip - *dir * head_size * 2.0 - perp2 * head_size;
        lines.push(DebugVertex { position: tip.to_array(), color: *color });
        lines.push(DebugVertex { position: base1.to_array(), color: *color });
        lines.push(DebugVertex { position: tip.to_array(), color: *color });
        lines.push(DebugVertex { position: base2.to_array(), color: *color });
        lines.push(DebugVertex { position: tip.to_array(), color: *color });
        lines.push(DebugVertex { position: base3.to_array(), color: *color });
        lines.push(DebugVertex { position: tip.to_array(), color: *color });
        lines.push(DebugVertex { position: base4.to_array(), color: *color });
    }

    // --- Scale axes (slightly darker, shorter, with box at end) ---
    let s_axes = [
        (Vec3::X, [0.8, 0.1, 0.1, 1.0]),
        (Vec3::Y, [0.1, 0.8, 0.1, 1.0]),
        (Vec3::Z, [0.1, 0.1, 0.8, 1.0]),
    ];
    for (dir, color) in &s_axes {
        let end = origin + *dir * SCALE_LEN;
        lines.push(DebugVertex { position: origin.to_array(), color: *color });
        lines.push(DebugVertex { position: end.to_array(), color: *color });
        // Box at end
        let box_s = 0.06;
        let perp = if dir.dot(Vec3::Y).abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        let perp2 = dir.cross(perp).normalize();
        let perp = perp.cross(*dir).normalize();
        let c = end;
        let corners: [Vec3; 8] = [
            c + perp * box_s + perp2 * box_s + *dir * box_s,
            c + perp * box_s + perp2 * box_s - *dir * box_s,
            c + perp * box_s - perp2 * box_s + *dir * box_s,
            c + perp * box_s - perp2 * box_s - *dir * box_s,
            c - perp * box_s + perp2 * box_s + *dir * box_s,
            c - perp * box_s + perp2 * box_s - *dir * box_s,
            c - perp * box_s - perp2 * box_s + *dir * box_s,
            c - perp * box_s - perp2 * box_s - *dir * box_s,
        ];
        let edges = [
            (0,1),(0,2),(0,4),(1,3),(1,5),(2,3),(2,6),(3,7),
            (4,5),(4,6),(5,7),(6,7),
        ];
        for (i,j) in &edges {
            lines.push(DebugVertex { position: corners[*i].to_array(), color: *color });
            lines.push(DebugVertex { position: corners[*j].to_array(), color: *color });
        }
    }

    // --- Rotation rings (circles in the perpendicular plane) ---
    let r_colors = [
        ([1.0, 0.5, 0.5, 1.0], Vec3::X), // X rotation ring in YZ plane
        ([0.5, 1.0, 0.5, 1.0], Vec3::Y), // Y rotation ring in XZ plane
        ([0.5, 0.5, 1.0, 1.0], Vec3::Z), // Z rotation ring in XY plane
    ];
    for (color, axis) in &r_colors {
        let perp1 = if axis.dot(Vec3::Y).abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        let perp2 = axis.cross(perp1).normalize();
        let perp1 = perp1.cross(*axis).normalize();
        let mut prev = origin + perp1 * ROT_RADIUS;
        for i in 1..=ROT_SEGMENTS {
            let angle = (i as f32 / ROT_SEGMENTS as f32) * std::f32::consts::TAU;
            let next = origin + (perp1 * angle.cos() + perp2 * angle.sin()) * ROT_RADIUS;
            lines.push(DebugVertex { position: prev.to_array(), color: *color });
            lines.push(DebugVertex { position: next.to_array(), color: *color });
            prev = next;
        }
    }

    lines
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

/// Squared distance from point `p` to a polyline defined by `points`.
fn point_polyline_dist_sq(p: Vec2, points: &[Vec2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in points.windows(2) {
        let d = point_segment_dist_sq(p, w[0], w[1]);
        if d < best {
            best = d;
        }
    }
    best
}

/// Detect which gizmo handle (if any) the mouse is hovering over.
/// Priority: Scale > Rotate > Translate (so smaller handles are picked first).
pub fn detect_hover(
    transform: &Transform,
    view: glam::Mat4,
    proj: glam::Mat4,
    mouse_x: f32,
    mouse_y: f32,
    width: f32,
    height: f32,
) -> Option<GizmoHandle> {
    let origin = transform.translation;
    let mouse = Vec2::new(mouse_x, mouse_y);

    // --- Scale handles ---
    let s_axes = [
        (GizmoAxis::X, Vec3::X),
        (GizmoAxis::Y, Vec3::Y),
        (GizmoAxis::Z, Vec3::Z),
    ];
    let mut best_scale: Option<(GizmoHandle, f32)> = None;
    for (axis, dir) in &s_axes {
        let end_screen = project_screen(origin + *dir * SCALE_LEN, view, proj, width, height);
        let origin_screen = project_screen(origin, view, proj, width, height);
        let d = point_segment_dist_sq(mouse, origin_screen, end_screen).sqrt();
        if d < HOVER_PIXEL_THRESHOLD {
            if best_scale.map_or(true, |(_, bd)| d < bd) {
                best_scale = Some((GizmoHandle::Scale(*axis), d));
            }
        }
    }
    if let Some((h, _)) = best_scale {
        return Some(h);
    }

    // --- Rotation rings ---
    let r_axes = [
        (GizmoAxis::X, Vec3::X),
        (GizmoAxis::Y, Vec3::Y),
        (GizmoAxis::Z, Vec3::Z),
    ];
    let mut best_rotate: Option<(GizmoHandle, f32)> = None;
    for (axis, axis_dir) in &r_axes {
        let perp1 = if axis_dir.dot(Vec3::Y).abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        let perp2 = axis_dir.cross(perp1).normalize();
        let perp1 = perp1.cross(*axis_dir).normalize();
        let mut ring_screen = Vec::with_capacity(ROT_SEGMENTS + 1);
        for i in 0..=ROT_SEGMENTS {
            let angle = (i as f32 / ROT_SEGMENTS as f32) * std::f32::consts::TAU;
            let pt = origin + (perp1 * angle.cos() + perp2 * angle.sin()) * ROT_RADIUS;
            ring_screen.push(project_screen(pt, view, proj, width, height));
        }
        let d = point_polyline_dist_sq(mouse, &ring_screen).sqrt();
        if d < HOVER_PIXEL_THRESHOLD {
            if best_rotate.map_or(true, |(_, bd)| d < bd) {
                best_rotate = Some((GizmoHandle::Rotate(*axis), d));
            }
        }
    }
    if let Some((h, _)) = best_rotate {
        return Some(h);
    }

    // --- Translate axes ---
    let t_axes = [
        (GizmoAxis::X, Vec3::X),
        (GizmoAxis::Y, Vec3::Y),
        (GizmoAxis::Z, Vec3::Z),
    ];
    let mut best_translate: Option<(GizmoHandle, f32)> = None;
    for (axis, dir) in &t_axes {
        let end_screen = project_screen(origin + *dir * TRANS_LEN, view, proj, width, height);
        let origin_screen = project_screen(origin, view, proj, width, height);
        let d = point_segment_dist_sq(mouse, origin_screen, end_screen).sqrt();
        if d < HOVER_PIXEL_THRESHOLD {
            if best_translate.map_or(true, |(_, bd)| d < bd) {
                best_translate = Some((GizmoHandle::Translate(*axis), d));
            }
        }
    }
    best_translate.map(|(h, _)| h)
}

/// Apply drag manipulation along/around the given handle based on mouse screen delta.
pub fn apply_drag(
    transform: &mut Transform,
    handle: GizmoHandle,
    mouse_delta: Vec2,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
    camera_pos: Vec3,
) {
    match handle {
        GizmoHandle::Translate(axis) => apply_drag_translate(transform, axis, mouse_delta, view, proj, width, height, camera_pos),
        GizmoHandle::Rotate(axis) => apply_drag_rotate(transform, axis, mouse_delta, view, proj, width, height, camera_pos),
        GizmoHandle::Scale(axis) => apply_drag_scale(transform, axis, mouse_delta, view, proj, width, height, camera_pos),
    }
}

fn apply_drag_translate(
    transform: &mut Transform,
    axis: GizmoAxis,
    mouse_delta: Vec2,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
    camera_pos: Vec3,
) {
    let axis_dir = axis_to_vec3(axis);
    let origin_screen = project_screen(transform.translation, view, proj, width, height);
    let end_screen = project_screen(transform.translation + axis_dir * TRANS_LEN, view, proj, width, height);
    let axis_screen = end_screen - origin_screen;
    if axis_screen.length_squared() < 1e-6 {
        return;
    }
    let axis_screen_dir = axis_screen.normalize();
    let screen_move = mouse_delta.dot(axis_screen_dir);
    let dist = (camera_pos - transform.translation).length().max(0.1);
    let world_scale = dist * 0.002;
    let world_move = screen_move * world_scale;
    match axis {
        GizmoAxis::X => transform.translation.x += world_move,
        GizmoAxis::Y => transform.translation.y += world_move,
        GizmoAxis::Z => transform.translation.z += world_move,
    }
}

fn apply_drag_rotate(
    transform: &mut Transform,
    axis: GizmoAxis,
    mouse_delta: Vec2,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
    camera_pos: Vec3,
) {
    // Project rotation ring center to screen
    let origin_screen = project_screen(transform.translation, view, proj, width, height);
    // Approximate: angle change proportional to tangential mouse movement around center
    let to_mouse = Vec2::new(
        origin_screen.x + mouse_delta.x,
        origin_screen.y + mouse_delta.y,
    ) - origin_screen;
    if to_mouse.length_squared() < 1e-6 {
        return;
    }
    // Use the perpendicular component of mouse delta as rotation amount
    let ring_r = ROT_RADIUS;
    let ring_r_screen = {
        let p1 = project_screen(transform.translation, view, proj, width, height);
        let p2 = project_screen(transform.translation + Vec3::X * ring_r, view, proj, width, height);
        (p2 - p1).length()
    };
    if ring_r_screen < 1.0 {
        return;
    }
    // Angle delta ≈ arc length / radius = pixel_delta / ring_r_screen
    // Use the larger of x/y delta for stability
    let pixel_delta = mouse_delta.length();
    let angle_delta = pixel_delta / ring_r_screen;
    // Sign: use cross product of mouse delta against a reference direction
    // Simplified: just use the magnitude with sign from movement direction
    let sign = {
        let axis_dir = axis_to_vec3(axis);
        let cam_to_obj = transform.translation - camera_pos;
        // Determine if we're viewing from + or - side of rotation axis
        let dot = cam_to_obj.dot(axis_dir);
        if dot > 0.0 { 1.0 } else { -1.0 }
    };
    // Use x delta for X/Y axis rotation, y for Z — simplified heuristic
    let signed_delta = angle_delta * sign * 0.5;

    let rot = match axis {
        GizmoAxis::X => Quat::from_axis_angle(Vec3::X, signed_delta),
        GizmoAxis::Y => Quat::from_axis_angle(Vec3::Y, signed_delta),
        GizmoAxis::Z => Quat::from_axis_angle(Vec3::Z, signed_delta),
    };
    transform.rotation = (rot * transform.rotation).normalize();
}

fn apply_drag_scale(
    transform: &mut Transform,
    axis: GizmoAxis,
    mouse_delta: Vec2,
    view: glam::Mat4,
    proj: glam::Mat4,
    width: f32,
    height: f32,
    camera_pos: Vec3,
) {
    let axis_dir = axis_to_vec3(axis);
    let origin_screen = project_screen(transform.translation, view, proj, width, height);
    let end_screen = project_screen(transform.translation + axis_dir * SCALE_LEN, view, proj, width, height);
    let axis_screen = end_screen - origin_screen;
    if axis_screen.length_squared() < 1e-6 {
        return;
    }
    let axis_screen_dir = axis_screen.normalize();
    let screen_move = mouse_delta.dot(axis_screen_dir);
    let dist = (camera_pos - transform.translation).length().max(0.1);
    let world_scale = dist * 0.002;
    let world_move = screen_move * world_scale;
    match axis {
        GizmoAxis::X => transform.scale.x = (transform.scale.x + world_move).max(0.01),
        GizmoAxis::Y => transform.scale.y = (transform.scale.y + world_move).max(0.01),
        GizmoAxis::Z => transform.scale.z = (transform.scale.z + world_move).max(0.01),
    }
}

fn axis_to_vec3(axis: GizmoAxis) -> Vec3 {
    match axis {
        GizmoAxis::X => Vec3::X,
        GizmoAxis::Y => Vec3::Y,
        GizmoAxis::Z => Vec3::Z,
    }
}

/// Query the world for the selected entity and its transform.
pub fn selected_entity_transform(world: &World) -> Option<(hecs::Entity, Transform)> {
    for (entity, transform, _) in world.query::<(hecs::Entity, &Transform, &Selected)>().iter() {
        return Some((entity, transform.clone()));
    }
    None
}
