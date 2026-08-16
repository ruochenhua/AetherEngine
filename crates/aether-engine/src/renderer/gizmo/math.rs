// Math helpers for gizmo interaction.

use super::GizmoAxis;
use glam::{Vec2, Vec3, Vec4};

pub(super) fn project_screen(
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

pub(super) fn point_segment_dist_sq(p: Vec2, a: Vec2, b: Vec2) -> f32 {
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

pub(super) fn point_polyline_dist_sq(p: Vec2, points: &[Vec2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in points.windows(2) {
        let d = point_segment_dist_sq(p, w[0], w[1]);
        if d < best {
            best = d;
        }
    }
    best
}

pub(super) fn axis_to_vec3(axis: GizmoAxis) -> Vec3 {
    match axis {
        GizmoAxis::X => Vec3::X,
        GizmoAxis::Y => Vec3::Y,
        GizmoAxis::Z => Vec3::Z,
    }
}
