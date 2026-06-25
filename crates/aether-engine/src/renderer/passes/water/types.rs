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
    /// Explicit padding to reach a 192-byte struct size.
    pub _pad0: f32,
    /// Explicit padding.
    pub _pad1: f32,
    /// Explicit padding.
    pub _pad2: f32,
    /// Explicit padding.
    pub _pad3: f32,
    /// Explicit padding.
    pub _pad4: f32,
    /// Explicit padding.
    pub _pad5: f32,
    /// Explicit padding.
    pub _pad6: f32,
    /// Explicit padding.
    pub _pad7: f32,
    /// Explicit padding.
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
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            _pad5: 0.0,
            _pad6: 0.0,
            _pad7: 0.0,
            _pad8: 0.0,
        }
    }
}
