use glam::{Vec3, Vec4};

/// PBR Material properties.
#[derive(Debug, Clone)]
pub struct Material {
    /// Base color (albedo).
    pub base_color: Vec4,
    /// Metallic factor (0.0 = dielectric, 1.0 = metallic).
    pub metallic: f32,
    /// Roughness factor (0.0 = smooth, 1.0 = rough).
    pub roughness: f32,
    /// Emissive color.
    pub emissive: Vec3,
    /// Emissive intensity.
    pub emissive_intensity: f32,
    /// Normal map scale.
    pub normal_scale: f32,
    /// AO strength.
    pub ao_strength: f32,
    /// Base color texture handle (optional).
    pub base_color_texture: Option<super::Handle<super::texture::CpuTexture>>,
    /// Metallic-roughness texture handle (optional).
    pub metallic_roughness_texture: Option<super::Handle<super::texture::CpuTexture>>,
    /// Normal map texture handle (optional).
    pub normal_texture: Option<super::Handle<super::texture::CpuTexture>>,
    /// Emissive texture handle (optional).
    pub emissive_texture: Option<super::Handle<super::texture::CpuTexture>>,
    /// AO texture handle (optional).
    pub ao_texture: Option<super::Handle<super::texture::CpuTexture>>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            metallic: 0.0,
            roughness: 0.5,
            emissive: Vec3::ZERO,
            emissive_intensity: 1.0,
            normal_scale: 1.0,
            ao_strength: 1.0,
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            emissive_texture: None,
            ao_texture: None,
        }
    }
}

impl Material {
    /// Create a default dielectric material (plastic-like).
    pub fn dielectric() -> Self {
        Self {
            metallic: 0.0,
            roughness: 0.5,
            ..Default::default()
        }
    }

    /// Create a default metallic material.
    pub fn metallic() -> Self {
        Self {
            metallic: 1.0,
            roughness: 0.3,
            base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
            ..Default::default()
        }
    }
}
