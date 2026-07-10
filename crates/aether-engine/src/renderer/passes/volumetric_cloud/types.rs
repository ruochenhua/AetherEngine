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
    /// Render parameters:
    /// x = max_render_dist, y = weather_scale, z = base_noise_scale, w = high_freq_noise_scale.
    pub render_params: glam::Vec4,
    /// Detail parameters:
    /// x = high_freq_uv_scale, y = high_freq_h_scale, z = cloud_type, w = cloud_top_offset.
    pub detail_params: glam::Vec4,
    /// Cloud color gradient: xyz = low_altitude_color, w unused.
    pub cloud_color_low: glam::Vec4,
    /// Cloud color gradient: xyz = high_altitude_color, w unused.
    pub cloud_color_high: glam::Vec4,
    /// Light color (rgb) and intensity factor (a).
    pub light_color: glam::Vec4,
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            cloud_bounds: glam::Vec4::new(80.0, 120.0, 0.5, 1.0),
            wind_time: glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
            render_params: glam::Vec4::new(30000.0, 0.00008, 0.0003, 0.003),
            detail_params: glam::Vec4::new(2.5, 1.0, 0.75, 500.0),
            cloud_color_low: glam::Vec4::new(0.92, 0.92, 0.95, 0.0),
            cloud_color_high: glam::Vec4::new(0.98, 0.98, 1.0, 0.0),
            light_color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
        }
    }
}
