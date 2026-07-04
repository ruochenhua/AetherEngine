//! Types and constants for the volumetric cloud pass.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

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
    /// Quality-dependent parameters: x = primary_steps, y = shadow_steps, z = g_forward, w = g_back.
    pub quality_params: glam::Vec4,
    /// Cloud color gradient: xyz = low_altitude_color, w unused.
    pub cloud_color_low: glam::Vec4,
    /// Cloud color gradient: xyz = high_altitude_color, w unused.
    pub cloud_color_high: glam::Vec4,
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            cloud_bounds: glam::Vec4::new(80.0, 120.0, 0.5, 1.0),
            wind_time: glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
            quality_params: glam::Vec4::new(64.0, 6.0, 0.85, 0.3),
            cloud_color_low: glam::Vec4::new(0.92, 0.92, 0.95, 0.0),
            cloud_color_high: glam::Vec4::new(0.98, 0.98, 1.0, 0.0),
        }
    }
}
