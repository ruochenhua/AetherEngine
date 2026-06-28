//! Water pass uniform and vertex types.
//!
//! Contains the [`WaterUniform`] struct that mirrors the GPU-side uniform
//! block consumed by the water shader.

/// GPU uniform data for the water shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterUniform {
    /// View-projection matrix.
    pub view_proj: glam::Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Shallow water color (rgb, a unused).
    pub water_color: glam::Vec4,
    /// Deep water color (rgb, a unused).
    pub deep_color: glam::Vec4,
    /// Wave direction on the XZ plane.
    pub wave_direction: glam::Vec2,
    /// Wave amplitude.
    pub wave_amplitude: f32,
    /// Wave wavelength.
    pub wave_wavelength: f32,
    /// Wave speed.
    pub wave_speed: f32,
    /// Wave steepness.
    pub wave_steepness: f32,
    /// Current animation time in seconds.
    pub time: f32,
    /// Water level (world-space Y).
    pub level: f32,
    /// Fresnel power.
    pub fresnel_power: f32,
    /// Refraction UV distortion scale.
    pub refraction_scale: f32,
    /// Reflection intensity multiplier.
    pub reflectivity: f32,
    /// UV scale for dudv/normal map tiling.
    pub texture_scale: f32,
    /// Distortion strength contributed by the dudv map.
    pub dudv_strength: f32,
    /// 1 if a dudv map is configured, 0 otherwise.
    pub has_dudv: u32,
    /// 1 if a normal map is configured, 0 otherwise.
    pub has_normal: u32,
    /// Strength of the normal-map perturbation (0 = geometry normal only).
    pub normal_strength: f32,
    /// Padding to align the following vec4 members.
    pub _pad0: f32,
    /// Padding.
    pub _pad1: f32,
    /// Padding.
    pub _pad2: f32,
    /// Padding.
    pub _pad3: f32,
    /// Direction toward the directional light (xyz, w unused).
    pub sun_direction: glam::Vec4,
    /// Directional light color multiplied by intensity (rgb, w unused).
    pub sun_color: glam::Vec4,
    /// Inverse view-projection matrix for reconstructing underwater world positions.
    pub inv_view_proj: glam::Mat4,
    /// Scale applied to water thickness when computing depth-based color absorption.
    pub depth_scale: f32,
    /// Exponent for the Blinn-Phong sun specular highlight.
    pub specular_power: f32,
    /// UV scale multiplier for the second animated texture layer.
    pub secondary_scale: f32,
    /// Padding to align the following vec2 members.
    pub _pad4: f32,
    /// Base UV flow speed for the first animated texture layer.
    pub flow_speed: glam::Vec2,
    /// UV flow speed for the second animated texture layer.
    pub flow_speed_2: glam::Vec2,
    /// Padding to reach a 336-byte struct size.
    pub _pad5: f32,
    /// Padding.
    pub _pad6: f32,
    /// Padding.
    pub _pad7: f32,
    /// Padding.
    pub _pad8: f32,
}

impl Default for WaterUniform {
    fn default() -> Self {
        Self {
            view_proj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::new(0.0, 0.0, 0.0, 0.0),
            water_color: glam::Vec4::new(0.0, 0.35, 0.45, 1.0),
            deep_color: glam::Vec4::new(0.0, 0.15, 0.25, 1.0),
            wave_direction: glam::Vec2::new(1.0, 0.5),
            wave_amplitude: 0.3,
            wave_wavelength: 8.0,
            wave_speed: 2.0,
            wave_steepness: 0.6,
            time: 0.0,
            level: 0.0,
            fresnel_power: 3.0,
            refraction_scale: 0.02,
            reflectivity: 0.6,
            texture_scale: 4.0,
            dudv_strength: 0.02,
            has_dudv: 0,
            has_normal: 0,
            normal_strength: 1.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            sun_direction: glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
            sun_color: glam::Vec4::new(1.0, 1.0, 1.0, 0.0),
            inv_view_proj: glam::Mat4::IDENTITY,
            depth_scale: 0.15,
            specular_power: 128.0,
            secondary_scale: 0.7,
            _pad4: 0.0,
            flow_speed: glam::Vec2::new(0.03, 0.01),
            flow_speed_2: glam::Vec2::new(-0.02, 0.015),
            _pad5: 0.0,
            _pad6: 0.0,
            _pad7: 0.0,
            _pad8: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WaterUniform;
    use std::mem::size_of;

    #[test]
    fn water_uniform_matches_gpu_layout() {
        // The WGSL struct is explicitly laid out to 336 bytes; this guards
        // against silent layout drift when fields are added or reordered.
        assert_eq!(size_of::<WaterUniform>(), 336);
    }
}
