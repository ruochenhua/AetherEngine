use super::{Cascade, CASCADE_COUNT, SHADOW_MAP_SIZE};
use crate::renderer::frame::RenderFrame;
use glam::{Mat4, Vec4Swizzles};

/// Compute wgpu-compatible orthographic projection (z ∈ [0, 1]).
pub(super) fn ortho_wgpu(
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    m.col_mut(0)[0] = 2.0 / (right - left);
    m.col_mut(1)[1] = 2.0 / (top - bottom);
    m.col_mut(2)[2] = -1.0 / (far - near);
    m.col_mut(3)[0] = -(right + left) / (right - left);
    m.col_mut(3)[1] = -(top + bottom) / (top - bottom);
    m.col_mut(3)[2] = -near / (far - near);
    m
}

/// Compute all cascades for the current frame.
pub fn compute_cascades(frame: &RenderFrame, light_dir: &glam::Vec3) -> [Cascade; CASCADE_COUNT] {
    let camera = &frame.camera;
    let view = camera.view_matrix();
    let proj = camera.projection_matrix(frame.aspect);
    let near = camera.near;
    let far = camera.far;
    let splits = split_distances(near, far, CASCADE_COUNT, 0.75);
    let mut cascades = [Cascade {
        view_proj: Mat4::IDENTITY,
        split_depth: 0.0,
    }; CASCADE_COUNT];
    let mut previous_far = near;
    for (index, split) in splits.iter().enumerate() {
        let cascade_far = *split;
        let corners = frustum_corners(&view, &proj, near, far, previous_far, cascade_far);
        cascades[index] = compute_cascade(light_dir, &corners, cascade_far, far);
        previous_far = cascade_far;
    }
    cascades
}

fn frustum_corners(
    view: &Mat4,
    proj: &Mat4,
    cam_near: f32,
    cam_far: f32,
    slice_near: f32,
    slice_far: f32,
) -> [glam::Vec3; 8] {
    let inv_view_proj = (proj.mul_mat4(view)).inverse();
    let mut corners = [glam::Vec3::ZERO; 8];
    let mut index = 0;
    for distance in [slice_near, slice_far] {
        let z_ndc = cam_far * (distance - cam_near) / (distance * (cam_far - cam_near));
        for y in [-1.0f32, 1.0] {
            for x in [-1.0f32, 1.0] {
                let world = inv_view_proj * glam::Vec4::new(x, y, z_ndc, 1.0);
                corners[index] = world.xyz() / world.w;
                index += 1;
            }
        }
    }
    corners
}

pub(super) fn split_distances(near: f32, far: f32, count: usize, lambda: f32) -> Vec<f32> {
    (1..=count)
        .map(|index| {
            let t = index as f32 / count as f32;
            let logarithmic = near * (far / near).powf(t);
            let uniform = near + (far - near) * t;
            lambda * logarithmic + (1.0 - lambda) * uniform
        })
        .collect()
}

fn compute_cascade(
    light_dir: &glam::Vec3,
    corners: &[glam::Vec3],
    split_depth: f32,
    camera_far: f32,
) -> Cascade {
    let up = if light_dir.x.abs() < 0.001 && light_dir.z.abs() < 0.001 {
        glam::Vec3::X
    } else {
        glam::Vec3::Y
    };
    let light_view = glam::camera::rh::view::look_at_mat4(glam::Vec3::ZERO, *light_dir, up);
    let mut min_ls = glam::Vec3::splat(f32::MAX);
    let mut max_ls = glam::Vec3::splat(f32::MIN);
    for corner in corners {
        let light_space = light_view.transform_point3a(glam::Vec3A::from(*corner));
        min_ls = min_ls.min(light_space.into());
        max_ls = max_ls.max(light_space.into());
    }
    min_ls.z -= camera_far;
    max_ls.z += camera_far * 0.5;
    let texel_size = (max_ls.x - min_ls.x) / SHADOW_MAP_SIZE as f32;
    min_ls.x -= texel_size * 2.0;
    max_ls.x += texel_size * 2.0;
    min_ls.y -= texel_size * 2.0;
    max_ls.y += texel_size * 2.0;
    let snap = |value: f32| (value / texel_size).floor() * texel_size;
    min_ls.x = snap(min_ls.x);
    max_ls.x = snap(max_ls.x);
    min_ls.y = snap(min_ls.y);
    max_ls.y = snap(max_ls.y);
    let projection = ortho_wgpu(min_ls.x, max_ls.x, min_ls.y, max_ls.y, -max_ls.z, -min_ls.z);
    Cascade {
        view_proj: projection * light_view,
        split_depth,
    }
}
