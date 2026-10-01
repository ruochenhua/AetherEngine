use glam::{Mat4, Vec3};

pub(super) fn transform_point(transform: Mat4, p: [f32; 3]) -> [f32; 3] {
    transform.transform_point3(Vec3::from_array(p)).to_array()
}

pub(super) fn transform_vector(transform: Mat4, v: [f32; 3]) -> [f32; 3] {
    // Normalize after transformation so normals stay unit-length under scale.
    // Non-uniform scales ideally use the inverse-transpose of the upper 3x3.
    transform
        .transform_vector3(Vec3::from_array(v))
        .normalize()
        .to_array()
}

pub(super) fn transform_tangent(transform: Mat4, tangent: [f32; 4]) -> [f32; 4] {
    let direction = Vec3::new(tangent[0], tangent[1], tangent[2]);
    let transformed = transform.transform_vector3(direction).normalize();
    [transformed.x, transformed.y, transformed.z, tangent[3]]
}
