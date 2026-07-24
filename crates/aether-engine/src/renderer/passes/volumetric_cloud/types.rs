//! Types and constants for the volumetric cloud pass.
//!
//! Layout mirrors the uniform block consumed by the inlined `shader::SHADER`
//! WGSL, which is a direct port of NadirRoGue/RenderEngine's
//! volumetricclouds.frag.

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
    /// Spherical cloud shell center (xyz) and inner radius (w).
    pub sphere_center_inner: glam::Vec4,
    /// x = outer radius, y = max render distance, z = cloud top offset, w = 0.
    pub sphere_outer_params: glam::Vec4,
    /// xyz = wind direction, w = time * cloud speed.
    pub wind_time: glam::Vec4,
    /// x = weather scale, y = base noise scale, z = high-freq noise scale, w = high-freq UV scale.
    pub noise_scales: glam::Vec4,
    /// x = high-freq H scale, y = cloud type, z = coverage multiplier, w = 0.
    pub detail_params: glam::Vec4,
    /// rgb = real light color, a = light factor.
    pub light_color: glam::Vec4,
    /// rgb = horizon color, a = 0.
    pub horizon_color: glam::Vec4,
    /// rgb = zenit color, a = 0.
    pub zenit_color: glam::Vec4,
    /// rgb = cloud color tint, a = 0.
    pub cloud_color: glam::Vec4,
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            sphere_center_inner: glam::Vec4::new(0.0, -6360.0, 0.0, 6440.0),
            sphere_outer_params: glam::Vec4::new(6480.0, 30000.0, 0.0, 0.0),
            wind_time: glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
            noise_scales: glam::Vec4::new(1.0, 1.0, 1.0, 150.0),
            detail_params: glam::Vec4::new(4.0, 0.5, 0.5, 0.0),
            light_color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            horizon_color: glam::Vec4::new(0.8, 0.85, 1.0, 0.0),
            zenit_color: glam::Vec4::new(0.0, 0.5, 1.0, 0.0),
            cloud_color: glam::Vec4::new(1.0, 1.0, 1.0, 0.0),
        }
    }
}
