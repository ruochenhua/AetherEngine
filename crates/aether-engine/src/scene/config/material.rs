//! Opaque PBR material configuration loaded from RON.

use serde::{Deserialize, Serialize};

/// Which texture channel supplies a material value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureChannel {
    /// Red channel.
    R,
    /// Green channel.
    G,
    /// Blue channel.
    B,
    /// Alpha channel.
    A,
    /// Constant zero.
    Zero,
    /// Constant one.
    One,
}

/// Explicit channel mapping for a packed occlusion/roughness/metallic texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrmSwizzle {
    /// Channel used for ambient occlusion.
    #[serde(default = "default_orm_ao")]
    pub ao: TextureChannel,
    /// Channel used for roughness.
    #[serde(default = "default_orm_roughness")]
    pub roughness: TextureChannel,
    /// Channel used for metallic.
    #[serde(default = "default_orm_metallic")]
    pub metallic: TextureChannel,
}

fn default_orm_ao() -> TextureChannel {
    TextureChannel::R
}

fn default_orm_roughness() -> TextureChannel {
    TextureChannel::G
}

fn default_orm_metallic() -> TextureChannel {
    TextureChannel::B
}

impl Default for OrmSwizzle {
    fn default() -> Self {
        Self {
            ao: default_orm_ao(),
            roughness: default_orm_roughness(),
            metallic: default_orm_metallic(),
        }
    }
}

/// PBR material parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    #[serde(default)]
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    #[serde(default)]
    pub metallic: f32,
    /// Render the object as a solid albedo color without lighting or shadows.
    #[serde(default)]
    pub unlit: bool,
    /// Optional albedo texture path.
    #[serde(default)]
    pub albedo_texture: Option<String>,
    /// Optional tangent-space normal texture path.
    #[serde(default)]
    pub normal_texture: Option<String>,
    /// Optional packed occlusion/roughness/metallic texture path.
    #[serde(default)]
    pub orm_texture: Option<String>,
    /// Optional emissive texture path.
    #[serde(default)]
    pub emissive_texture: Option<String>,
    /// Normal map strength in the inclusive range [0, 2].
    #[serde(default = "default_normal_scale")]
    pub normal_scale: f32,
    /// Occlusion contribution in the inclusive range [0, 1].
    #[serde(default = "default_occlusion_strength")]
    pub occlusion_strength: f32,
    /// Emissive color multiplier in linear RGB.
    #[serde(default)]
    pub emissive: [f32; 3],
    /// Emissive intensity, which must be non-negative.
    #[serde(default = "default_emissive_intensity")]
    pub emissive_intensity: f32,
    /// Explicit channel mapping for the ORM texture.
    #[serde(default)]
    pub orm_swizzle: OrmSwizzle,
}

fn default_albedo() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}

fn default_normal_scale() -> f32 {
    1.0
}

fn default_occlusion_strength() -> f32 {
    1.0
}

fn default_emissive_intensity() -> f32 {
    1.0
}

impl Default for MaterialConfig {
    fn default() -> Self {
        Self {
            albedo: default_albedo(),
            roughness: 0.5,
            metallic: 0.0,
            unlit: false,
            albedo_texture: None,
            normal_texture: None,
            orm_texture: None,
            emissive_texture: None,
            normal_scale: default_normal_scale(),
            occlusion_strength: default_occlusion_strength(),
            emissive: [0.0; 3],
            emissive_intensity: default_emissive_intensity(),
            orm_swizzle: OrmSwizzle::default(),
        }
    }
}
