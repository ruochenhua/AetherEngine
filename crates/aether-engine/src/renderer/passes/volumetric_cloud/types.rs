//! Types and constants for the volumetric cloud pass.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

/// Size of the generated 3D noise texture (NxNxN).
pub(super) const NOISE_SIZE: u32 = 64;

/// GPU uniform data for the volumetric cloud shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CloudUniform {
    /// Inverse view-projection matrix for view-ray reconstruction.
    pub inv_view_proj: Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Direction toward the sun (xyz, w unused).
    pub sun_direction: glam::Vec4,
    /// Cloud slab bounds and density: x=min_y, y=max_y, z=coverage, w=density.
    pub cloud_bounds: glam::Vec4,
    /// Wind direction (xyz) and current time (w).
    pub wind_time: glam::Vec4,
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            cloud_bounds: glam::Vec4::new(80.0, 120.0, 0.5, 1.0),
            wind_time: glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        }
    }
}
