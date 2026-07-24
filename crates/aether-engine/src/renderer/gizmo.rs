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
        lines.push(DebugVertex {
            position: origin.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: end.to_array(),
            color: *color,
        });
        // Arrow head (small pyramid hint: two short perpendicular lines)
        let head_size = 0.08;
        let perp1 = if dir.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let perp2 = dir.cross(perp1).normalize();
        // Same fix as the rotation rings below: derive the in-plane basis
        // from perp2, not perp1 — crossing the original arbitrary vector
        // with `dir` again would yield a vector anti-parallel to perp2,
        // collapsing the four head corners into two.
        let perp1 = perp2.cross(*dir).normalize();
        let tip = end;
        let base1 = tip - *dir * head_size * 2.0 + perp1 * head_size;
        let base2 = tip - *dir * head_size * 2.0 - perp1 * head_size;
        let base3 = tip - *dir * head_size * 2.0 + perp2 * head_size;
        let base4 = tip - *dir * head_size * 2.0 - perp2 * head_size;
        lines.push(DebugVertex {
            position: tip.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: base1.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: tip.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: base2.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: tip.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: base3.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: tip.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: base4.to_array(),
            color: *color,
        });
    }

    // --- Scale axes (slightly darker, shorter, with box at end) ---
    let s_axes = [
        (Vec3::X, [0.8, 0.1, 0.1, 1.0]),
        (Vec3::Y, [0.1, 0.8, 0.1, 1.0]),
        (Vec3::Z, [0.1, 0.1, 0.8, 1.0]),
    ];
    for (dir, color) in &s_axes {
        let end = origin + *dir * SCALE_LEN;
        lines.push(DebugVertex {
            position: origin.to_array(),
            color: *color,
        });
        lines.push(DebugVertex {
            position: end.to_array(),
            color: *color,
        });
        // Box at end
        let box_s = 0.06;
        let perp = if dir.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let perp2 = dir.cross(perp).normalize();
        // Same fix as the rotation rings below: derive the in-plane basis
        // from perp2, not perp — crossing the original arbitrary vector
        // with `dir` again would yield a vector anti-parallel to perp2,
        // collapsing the box into a flat rectangle.
        let perp = perp2.cross(*dir).normalize();
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
            (0, 1),
            (0, 2),
            (0, 4),
            (1, 3),
            (1, 5),
            (2, 3),
            (2, 6),
            (3, 7),
            (4, 5),
            (4, 6),
            (5, 7),
            (6, 7),
        ];
        for (i, j) in &edges {
            lines.push(DebugVertex {
                position: corners[*i].to_array(),
                color: *color,
            });
            lines.push(DebugVertex {
                position: corners[*j].to_array(),
                color: *color,
            });
        }
    }

    // --- Rotation rings (circles in the perpendicular plane) ---
    let r_colors = [
        ([1.0, 0.5, 0.5, 1.0], Vec3::X), // X rotation ring in YZ plane
        ([0.5, 1.0, 0.5, 1.0], Vec3::Y), // Y rotation ring in XZ plane
        ([0.5, 0.5, 1.0, 1.0], Vec3::Z), // Z rotation ring in XY plane
    ];
    for (color, axis) in &r_colors {
        let perp1 = if axis.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let perp2 = axis.cross(perp1).normalize();
        // Build the in-plane basis vector from perp2 (not perp1): crossing the
        // original arbitrary vector with `axis` again would yield a vector
        // anti-parallel to perp2, collapsing the ring into a straight line.
        let perp1 = perp2.cross(*axis).normalize();
        let mut prev = origin + perp1 * ROT_RADIUS;
        for i in 1..=ROT_SEGMENTS {
            let angle = (i as f32 / ROT_SEGMENTS as f32) * std::f32::consts::TAU;
            let next = origin + (perp1 * angle.cos() + perp2 * angle.sin()) * ROT_RADIUS;
            lines.push(DebugVertex {
                position: prev.to_array(),
                color: *color,
            });
            lines.push(DebugVertex {
                position: next.to_array(),
                color: *color,
            });
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
        if d < HOVER_PIXEL_THRESHOLD && best_scale.map_or(true, |(_, bd)| d < bd) {
            best_scale = Some((GizmoHandle::Scale(*axis), d));
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
        let perp1 = if axis_dir.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let perp2 = axis_dir.cross(perp1).normalize();
        // See build_transform_gizmo: derive the in-plane basis from perp2, not
        // from the original arbitrary vector, or the ring degenerates to a line.
        let perp1 = perp2.cross(*axis_dir).normalize();
        let mut ring_screen = Vec::with_capacity(ROT_SEGMENTS + 1);
        for i in 0..=ROT_SEGMENTS {
            let angle = (i as f32 / ROT_SEGMENTS as f32) * std::f32::consts::TAU;
            let pt = origin + (perp1 * angle.cos() + perp2 * angle.sin()) * ROT_RADIUS;
            ring_screen.push(project_screen(pt, view, proj, width, height));
        }
        let d = point_polyline_dist_sq(mouse, &ring_screen).sqrt();
        if d < HOVER_PIXEL_THRESHOLD && best_rotate.map_or(true, |(_, bd)| d < bd) {
            best_rotate = Some((GizmoHandle::Rotate(*axis), d));
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
        if d < HOVER_PIXEL_THRESHOLD && best_translate.map_or(true, |(_, bd)| d < bd) {
            best_translate = Some((GizmoHandle::Translate(*axis), d));
        }
    }
    best_translate.map(|(h, _)| h)
}

/// Camera context bundled for gizmo drag operations (avoids 8-arg functions).
pub struct GizmoCameraCtx {
    /// View matrix.
    pub view: glam::Mat4,
    /// Projection matrix.
    pub proj: glam::Mat4,
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels.
    pub height: f32,
    /// Camera position in world space.
    pub camera_pos: Vec3,
}

/// Apply drag manipulation along/around the given handle based on mouse screen delta.
pub fn apply_drag(
    transform: &mut Transform,
    handle: GizmoHandle,
    mouse_delta: Vec2,
    cam: &GizmoCameraCtx,
) {
    match handle {
        GizmoHandle::Translate(axis) => apply_drag_translate(transform, axis, mouse_delta, cam),
        GizmoHandle::Rotate(axis) => apply_drag_rotate(transform, axis, mouse_delta, cam),
        GizmoHandle::Scale(axis) => apply_drag_scale(transform, axis, mouse_delta, cam),
    }
}

fn apply_drag_translate(
    transform: &mut Transform,
    axis: GizmoAxis,
    mouse_delta: Vec2,
    cam: &GizmoCameraCtx,
) {
    let axis_dir = axis_to_vec3(axis);
    let origin_screen = project_screen(
        transform.translation,
        cam.view,
        cam.proj,
        cam.width,
        cam.height,
    );
    let end_screen = project_screen(
        transform.translation + axis_dir * TRANS_LEN,
        cam.view,
        cam.proj,
        cam.width,
        cam.height,
    );
    let axis_screen = end_screen - origin_screen;
    if axis_screen.length_squared() < 1e-6 {
        return;
    }
    let axis_screen_dir = axis_screen.normalize();
    let screen_move = mouse_delta.dot(axis_screen_dir);
    let dist = (cam.camera_pos - transform.translation).length().max(0.1);
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
    cam: &GizmoCameraCtx,
) {
    // Project rotation ring center to screen
    let origin_screen = project_screen(
        transform.translation,
        cam.view,
        cam.proj,
        cam.width,
        cam.height,
    );
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
        let p1 = project_screen(
            transform.translation,
            cam.view,
            cam.proj,
            cam.width,
            cam.height,
        );
        let p2 = project_screen(
            transform.translation + Vec3::X * ring_r,
            cam.view,
            cam.proj,
            cam.width,
            cam.height,
        );
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
        let cam_to_obj = transform.translation - cam.camera_pos;
        // Determine if we're viewing from + or - side of rotation axis
        let dot = cam_to_obj.dot(axis_dir);
        if dot > 0.0 {
            1.0
        } else {
            -1.0
        }
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
    cam: &GizmoCameraCtx,
) {
    let axis_dir = axis_to_vec3(axis);
    let origin_screen = project_screen(
        transform.translation,
        cam.view,
        cam.proj,
        cam.width,
        cam.height,
    );
    let end_screen = project_screen(
        transform.translation + axis_dir * SCALE_LEN,
        cam.view,
        cam.proj,
        cam.width,
        cam.height,
    );
    let axis_screen = end_screen - origin_screen;
    if axis_screen.length_squared() < 1e-6 {
        return;
    }
    let axis_screen_dir = axis_screen.normalize();
    let screen_move = mouse_delta.dot(axis_screen_dir);
    let dist = (cam.camera_pos - transform.translation).length().max(0.1);
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
    world
        .query::<(hecs::Entity, &Transform, &Selected)>()
        .iter()
        .next()
        .map(|(entity, transform, _)| (entity, transform.clone()))
}

/// Fetch a specific entity's transform, if the entity exists and has one.
///
/// Used by the editor to anchor the gizmo to the last selected entity of a
/// multi-selection instead of an arbitrary `Selected` match.
pub fn entity_transform(world: &World, entity: hecs::Entity) -> Option<Transform> {
    world
        .query_one::<&Transform>(entity)
        .get()
        .ok()
        .map(|transform| transform.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 800.0;
    const H: f32 = 600.0;

    /// Build a right-handed camera looking at the origin (where the test gizmo sits).
    fn camera(eye: Vec3, up: Vec3) -> (glam::Mat4, glam::Mat4, Vec3) {
        let view = glam::Mat4::look_at_rh(eye, Vec3::ZERO, up);
        let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), W / H, 0.1, 100.0);
        (view, proj, eye)
    }

    /// Isometric-ish 3/4 view: all three axes and all three rings are separated
    /// on screen, so no ring projects edge-on onto an axis line.
    fn iso() -> (glam::Mat4, glam::Mat4, Vec3) {
        camera(Vec3::new(4.0, 3.0, 4.0), Vec3::Y)
    }

    fn ctx(view: glam::Mat4, proj: glam::Mat4, eye: Vec3) -> GizmoCameraCtx {
        GizmoCameraCtx {
            view,
            proj,
            width: W,
            height: H,
            camera_pos: eye,
        }
    }

    /// Project a world point to screen pixels using the same helper as the code
    /// under test, so hover points land exactly on the intended gizmo feature.
    fn scr(p: Vec3, view: glam::Mat4, proj: glam::Mat4) -> Vec2 {
        project_screen(p, view, proj, W, H)
    }

    /// On-screen unit direction of a gizmo axis emanating from the origin.
    fn axis_screen_dir(axis: Vec3, len: f32, view: glam::Mat4, proj: glam::Mat4) -> Vec2 {
        (scr(axis * len, view, proj) - scr(Vec3::ZERO, view, proj)).normalize()
    }

    // ── detect_hover: translate shafts ──────────────────────────────

    #[test]
    fn hover_translate_x_axis() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        // 1.35 is on the translate shaft (0..1.5) but past the scale box (0..1.2).
        let m = scr(Vec3::X * 1.35, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Translate(GizmoAxis::X)),
            "hovering the +X arrow shaft should select Translate(X), got {got:?}"
        );
    }

    #[test]
    fn hover_translate_y_axis() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        let m = scr(Vec3::Y * 1.35, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Translate(GizmoAxis::Y)),
            "hovering the +Y arrow shaft should select Translate(Y), got {got:?}"
        );
    }

    #[test]
    fn hover_translate_z_axis() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        let m = scr(Vec3::Z * 1.35, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Translate(GizmoAxis::Z)),
            "hovering the +Z arrow shaft should select Translate(Z), got {got:?}"
        );
    }

    // ── detect_hover: scale boxes (priority over translate) ─────────

    #[test]
    fn hover_scale_x_axis_prefers_scale_over_translate() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        // 0.6 lies on BOTH the scale segment (0..1.2) and the translate shaft
        // (0..1.5); the documented priority must pick the smaller Scale handle.
        let m = scr(Vec3::X * 0.6, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Scale(GizmoAxis::X)),
            "overlapping scale/translate on +X should prefer Scale(X), got {got:?}"
        );
    }

    #[test]
    fn hover_scale_y_axis_prefers_scale_over_translate() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        let m = scr(Vec3::Y * 0.6, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Scale(GizmoAxis::Y)),
            "overlapping scale/translate on +Y should prefer Scale(Y), got {got:?}"
        );
    }

    #[test]
    fn hover_scale_z_axis_prefers_scale_over_translate() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        let m = scr(Vec3::Z * 0.6, view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Scale(GizmoAxis::Z)),
            "overlapping scale/translate on +Z should prefer Scale(Z), got {got:?}"
        );
    }

    // ── detect_hover: rotation rings (viewed face-on) ───────────────

    #[test]
    fn hover_rotate_x_axis() {
        // Camera on +X so the X rotation ring (YZ plane) is seen face-on.
        let (view, proj, _) = camera(Vec3::new(5.0, 0.0, 0.0), Vec3::Y);
        let t = Transform::default();
        // 45 degrees around the ring, clear of the Y and Z axes.
        let c = ROT_RADIUS * std::f32::consts::FRAC_1_SQRT_2;
        let m = scr(Vec3::new(0.0, c, c), view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Rotate(GizmoAxis::X)),
            "hovering the X ring should select Rotate(X), got {got:?}"
        );
    }

    #[test]
    fn hover_rotate_y_axis() {
        // Camera on +Y so the Y rotation ring (XZ plane) is seen face-on.
        // Up must not be parallel to the view direction, so use +Z.
        let (view, proj, _) = camera(Vec3::new(0.0, 5.0, 0.0), Vec3::Z);
        let t = Transform::default();
        let c = ROT_RADIUS * std::f32::consts::FRAC_1_SQRT_2;
        let m = scr(Vec3::new(c, 0.0, c), view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Rotate(GizmoAxis::Y)),
            "hovering the Y ring should select Rotate(Y), got {got:?}"
        );
    }

    #[test]
    fn hover_rotate_z_axis() {
        // Camera on +Z so the Z rotation ring (XY plane) is seen face-on.
        let (view, proj, _) = camera(Vec3::new(0.0, 0.0, 5.0), Vec3::Y);
        let t = Transform::default();
        let c = ROT_RADIUS * std::f32::consts::FRAC_1_SQRT_2;
        let m = scr(Vec3::new(c, c, 0.0), view, proj);
        let got = detect_hover(&t, view, proj, m.x, m.y, W, H);
        assert_eq!(
            got,
            Some(GizmoHandle::Rotate(GizmoAxis::Z)),
            "hovering the Z ring should select Rotate(Z), got {got:?}"
        );
    }

    #[test]
    fn hover_far_from_gizmo_returns_none() {
        let (view, proj, _) = iso();
        let t = Transform::default();
        // Top-left corner pixel, hundreds of px from any gizmo feature.
        let got = detect_hover(&t, view, proj, 5.0, 5.0, W, H);
        assert_eq!(
            got, None,
            "a pixel far from the gizmo should hover nothing, got {got:?}"
        );
    }

    // ── build_transform_gizmo: rotation rings ──────────────────────

    #[test]
    fn rotation_rings_are_circular_not_degenerate() {
        // Regression: a wrong perpendicular basis collapsed every ring onto a
        // single axis, producing a straight line whose points sit at varying
        // distances from the origin. A correct ring keeps every vertex exactly
        // at ROT_RADIUS. The rings are appended last, in X/Y/Z order.
        let lines = build_transform_gizmo(&Transform::default());
        let ring_vert_count = 3 * ROT_SEGMENTS * 2;
        let ring_verts = &lines[lines.len() - ring_vert_count..];
        assert_eq!(ring_verts.len(), ring_vert_count);
        for v in ring_verts {
            let r = Vec3::from_array(v.position).length();
            assert!(
                (r - ROT_RADIUS).abs() < 1e-3,
                "every ring vertex should lie at radius {ROT_RADIUS}, got {r} \
                 (the ring collapsed into a line)"
            );
        }
    }

    // ── build_transform_gizmo: arrow heads ─────────────────────────

    #[test]
    fn arrow_heads_have_four_distinct_base_corners_around_axis() {
        // Regression: the arrow head used the same wrong perpendicular basis
        // as the rotation rings (perp1 anti-parallel to perp2), collapsing the
        // four base corners into two. The head must be a cross of four
        // distinct corners, each at distance `head_size` from the axis and
        // mirrored about it. Per axis the builder emits 2 line verts plus
        // 8 head verts (tip, base) × 4; axes run in X/Y/Z order.
        const HEAD_SIZE: f32 = 0.08;
        let lines = build_transform_gizmo(&Transform::default());
        let dirs = [Vec3::X, Vec3::Y, Vec3::Z];
        for (axis, dir) in dirs.iter().enumerate() {
            let section = &lines[axis * 10..axis * 10 + 10];
            let tip = Vec3::from_array(section[2].position);
            assert!(
                (tip - *dir * TRANS_LEN).length() < 1e-4,
                "arrow tip should sit at the axis end"
            );
            let bases: [Vec3; 4] = [
                Vec3::from_array(section[3].position),
                Vec3::from_array(section[5].position),
                Vec3::from_array(section[7].position),
                Vec3::from_array(section[9].position),
            ];
            // Four distinct corners.
            for (i, b) in bases.iter().enumerate() {
                for (j, other) in bases.iter().enumerate() {
                    if i != j {
                        assert!(
                            (*b - *other).length() > 1e-4,
                            "axis {axis}: base corners {i} and {j} coincide at {b:?} \
                             (the head collapsed into a single line)"
                        );
                    }
                }
                // Every corner at distance HEAD_SIZE from the axis line.
                let rel = *b - tip;
                let off_axis = rel - *dir * rel.dot(*dir);
                assert!(
                    (off_axis.length() - HEAD_SIZE).abs() < 1e-4,
                    "axis {axis}: base corner {i} should be {HEAD_SIZE} off the axis, \
                     got {}",
                    off_axis.length()
                );
            }
            // Mirrored about the axis: opposite corners sum to a point on it,
            // and the two cross arms are orthogonal.
            let arm1 = bases[0] - bases[1];
            let arm2 = bases[2] - bases[3];
            assert!(
                arm1.dot(*dir).abs() < 1e-4 && arm2.dot(*dir).abs() < 1e-4,
                "axis {axis}: cross arms must be perpendicular to the axis"
            );
            assert!(
                arm1.dot(arm2).abs() < 1e-4,
                "axis {axis}: cross arms must be orthogonal to each other, dot = {}",
                arm1.dot(arm2)
            );
        }
    }

    // ── build_transform_gizmo: scale boxes ─────────────────────────

    #[test]
    fn scale_boxes_have_eight_center_symmetric_corners() {
        // Regression: the scale box used the same wrong perpendicular basis
        // as the rotation rings, collapsing the 8 cube corners into 6 (a flat
        // rectangle). A correct box has 8 distinct corners, all at distance
        // box_s·√3 from the box center and centrally symmetric about it.
        // The scale sections follow the translate sections: per axis 2 line
        // verts plus 12 edges × 2 verts, axes in X/Y/Z order.
        const BOX_S: f32 = 0.06;
        let lines = build_transform_gizmo(&Transform::default());
        let dirs = [Vec3::X, Vec3::Y, Vec3::Z];
        for (axis, dir) in dirs.iter().enumerate() {
            let start = 30 + axis * 26;
            let section = &lines[start..start + 26];
            let center = *dir * SCALE_LEN;
            // Collect the unique corners from the 12 edge vertex pairs.
            let mut corners: Vec<Vec3> = Vec::new();
            for v in &section[2..] {
                let p = Vec3::from_array(v.position);
                if corners.iter().all(|c| (*c - p).length() > 1e-4) {
                    corners.push(p);
                }
            }
            assert_eq!(
                corners.len(),
                8,
                "axis {axis}: box should have 8 distinct corners, got {} \
                 (the box collapsed into a flat rectangle)",
                corners.len()
            );
            let expected_r = BOX_S * 3.0f32.sqrt();
            for (i, c) in corners.iter().enumerate() {
                let r = (*c - center).length();
                assert!(
                    (r - expected_r).abs() < 1e-4,
                    "axis {axis}: corner {i} should be {expected_r} from the box \
                     center, got {r}"
                );
                // Central symmetry: the antipodal point must also be a corner.
                let antipodal = 2.0 * center - *c;
                assert!(
                    corners.iter().any(|q| (*q - antipodal).length() < 1e-4),
                    "axis {axis}: corner {i} has no antipodal corner (not symmetric \
                     about the box center)"
                );
            }
        }
    }

    // ── apply_drag: translate ───────────────────────────────────────

    #[test]
    fn drag_translate_x_moves_only_x() {
        let (view, proj, eye) = iso();
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        let dir = axis_screen_dir(Vec3::X, TRANS_LEN, view, proj);
        apply_drag(&mut t, GizmoHandle::Translate(GizmoAxis::X), dir * 100.0, &cam);

        let expected = 100.0 * eye.length() * 0.002;
        assert!(
            (t.translation.x - expected).abs() < 1e-3,
            "dragging +X by 100px should move x by ~{expected}, got {}",
            t.translation.x
        );
        assert_eq!(t.translation.y, 0.0, "y must be untouched");
        assert_eq!(t.translation.z, 0.0, "z must be untouched");
    }

    #[test]
    fn drag_translate_y_moves_only_y() {
        let (view, proj, eye) = iso();
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        let dir = axis_screen_dir(Vec3::Y, TRANS_LEN, view, proj);
        apply_drag(&mut t, GizmoHandle::Translate(GizmoAxis::Y), dir * 100.0, &cam);

        let expected = 100.0 * eye.length() * 0.002;
        assert!(
            (t.translation.y - expected).abs() < 1e-3,
            "dragging +Y by 100px should move y by ~{expected}, got {}",
            t.translation.y
        );
        assert_eq!(t.translation.x, 0.0, "x must be untouched");
        assert_eq!(t.translation.z, 0.0, "z must be untouched");
    }

    #[test]
    fn drag_translate_z_moves_only_z() {
        let (view, proj, eye) = iso();
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        let dir = axis_screen_dir(Vec3::Z, TRANS_LEN, view, proj);
        apply_drag(&mut t, GizmoHandle::Translate(GizmoAxis::Z), dir * 100.0, &cam);

        let expected = 100.0 * eye.length() * 0.002;
        assert!(
            (t.translation.z - expected).abs() < 1e-3,
            "dragging +Z by 100px should move z by ~{expected}, got {}",
            t.translation.z
        );
        assert_eq!(t.translation.x, 0.0, "x must be untouched");
        assert_eq!(t.translation.y, 0.0, "y must be untouched");
    }

    // ── apply_drag: rotate ──────────────────────────────────────────

    #[test]
    fn drag_rotate_x_changes_rotation_not_position() {
        let (view, proj, eye) = camera(Vec3::new(0.0, 0.0, 5.0), Vec3::Y);
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        apply_drag(&mut t, GizmoHandle::Rotate(GizmoAxis::X), Vec2::new(50.0, 0.0), &cam);

        assert_ne!(
            t.rotation,
            Quat::IDENTITY,
            "a rotate drag should change the rotation"
        );
        assert!(
            t.rotation.x.abs() > 1e-4,
            "Rotate(X) should produce a rotation about X, got {:?}",
            t.rotation
        );
        assert!(
            t.rotation.y.abs() < 1e-6 && t.rotation.z.abs() < 1e-6,
            "Rotate(X) must not twist about Y or Z, got {:?}",
            t.rotation
        );
        assert_eq!(t.translation, Vec3::ZERO, "position must not move");
        assert_eq!(t.scale, Vec3::ONE, "scale must not change");
    }

    #[test]
    fn drag_rotate_y_changes_rotation_not_position() {
        let (view, proj, eye) = camera(Vec3::new(0.0, 0.0, 5.0), Vec3::Y);
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        apply_drag(&mut t, GizmoHandle::Rotate(GizmoAxis::Y), Vec2::new(50.0, 0.0), &cam);

        assert_ne!(t.rotation, Quat::IDENTITY, "a rotate drag should change the rotation");
        assert!(
            t.rotation.y.abs() > 1e-4,
            "Rotate(Y) should produce a rotation about Y, got {:?}",
            t.rotation
        );
        assert!(
            t.rotation.x.abs() < 1e-6 && t.rotation.z.abs() < 1e-6,
            "Rotate(Y) must not twist about X or Z, got {:?}",
            t.rotation
        );
        assert_eq!(t.translation, Vec3::ZERO, "position must not move");
        assert_eq!(t.scale, Vec3::ONE, "scale must not change");
    }

    #[test]
    fn drag_rotate_z_changes_rotation_not_position() {
        let (view, proj, eye) = camera(Vec3::new(0.0, 0.0, 5.0), Vec3::Y);
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        apply_drag(&mut t, GizmoHandle::Rotate(GizmoAxis::Z), Vec2::new(50.0, 0.0), &cam);

        assert_ne!(t.rotation, Quat::IDENTITY, "a rotate drag should change the rotation");
        assert!(
            t.rotation.z.abs() > 1e-4,
            "Rotate(Z) should produce a rotation about Z, got {:?}",
            t.rotation
        );
        assert!(
            t.rotation.x.abs() < 1e-6 && t.rotation.y.abs() < 1e-6,
            "Rotate(Z) must not twist about X or Y, got {:?}",
            t.rotation
        );
        assert_eq!(t.translation, Vec3::ZERO, "position must not move");
        assert_eq!(t.scale, Vec3::ONE, "scale must not change");
    }

    // ── apply_drag: scale ───────────────────────────────────────────

    #[test]
    fn drag_scale_x_scales_only_x() {
        let (view, proj, eye) = iso();
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        let dir = axis_screen_dir(Vec3::X, SCALE_LEN, view, proj);
        apply_drag(&mut t, GizmoHandle::Scale(GizmoAxis::X), dir * 100.0, &cam);

        assert!(
            t.scale.x > 1.0,
            "dragging +X scale should grow x above 1.0, got {}",
            t.scale.x
        );
        assert_eq!(t.scale.y, 1.0, "y scale must be untouched");
        assert_eq!(t.scale.z, 1.0, "z scale must be untouched");
        assert_eq!(t.translation, Vec3::ZERO, "position must not move");
        assert_eq!(t.rotation, Quat::IDENTITY, "rotation must not change");
    }

    #[test]
    fn drag_scale_clamps_to_minimum() {
        let (view, proj, eye) = iso();
        let cam = ctx(view, proj, eye);
        let mut t = Transform::default();
        let dir = axis_screen_dir(Vec3::X, SCALE_LEN, view, proj);
        // Drag hard against the axis: without the clamp the scale would go negative.
        apply_drag(&mut t, GizmoHandle::Scale(GizmoAxis::X), dir * -100000.0, &cam);

        assert_eq!(
            t.scale.x, 0.01,
            "scale must be clamped to the 0.01 minimum, got {}",
            t.scale.x
        );
        assert_eq!(t.scale.y, 1.0, "y scale must be untouched");
        assert_eq!(t.scale.z, 1.0, "z scale must be untouched");
    }
}
