//! Math utilities and type aliases.
//!
//! Re-exports `glam` types and provides engine-specific extensions.

pub use glam::*;

/// Convert a `glam::Mat4` to a raw array for GPU uniform buffers.
pub fn mat4_to_array(m: &Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// Convert a `glam::Vec3` to a raw array.
pub fn vec3_to_array(v: Vec3) -> [f32; 3] {
    v.to_array()
}

/// Convert a `glam::Vec4` to a raw array.
pub fn vec4_to_array(v: Vec4) -> [f32; 4] {
    v.to_array()
}

/// Create a standard perspective projection matrix.
pub fn perspective_projection(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fov_y_rad, aspect, near, far)
}

/// Create a look-at view matrix.
pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, center, up)
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// Create an AABB from min/max corners.
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Compute the center of the AABB.
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Compute the half-extents (size / 2).
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
}
