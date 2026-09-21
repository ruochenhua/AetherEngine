//! Material schema resolution.

use crate::asset::texture::CpuTexture;
use crate::asset::{AssetManager, Handle};
use crate::scene::config::material::{MaterialConfig, OrmSwizzle};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Texture color interpretation used by the material resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSpace {
    /// Color data that is decoded from sRGB to linear before shading.
    Srgb,
    /// Data sampled without color conversion.
    Linear,
}

/// The four texture slots in the opaque material contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureUsage {
    /// Base color texture.
    Albedo,
    /// Tangent-space normal texture.
    Normal,
    /// Packed occlusion/roughness/metallic texture.
    Orm,
    /// Emissive color texture.
    Emissive,
}

impl TextureUsage {
    /// Return the fixed color space for this usage.
    pub const fn color_space(self) -> ColorSpace {
        match self {
            Self::Albedo | Self::Emissive => ColorSpace::Srgb,
            Self::Normal | Self::Orm => ColorSpace::Linear,
        }
    }

    const fn fallback(self) -> TextureFallback {
        match self {
            Self::Albedo | Self::Orm => TextureFallback::White,
            Self::Normal => TextureFallback::FlatNormal,
            Self::Emissive => TextureFallback::Black,
        }
    }
}

/// Stable key for a decoded texture request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextureCacheKey {
    canonical_path: String,
    usage: TextureUsage,
    color_space: ColorSpace,
}

impl TextureCacheKey {
    /// Build a key from an already normalized path and usage.
    pub fn new(path: impl Into<String>, usage: TextureUsage) -> Self {
        Self {
            canonical_path: path.into(),
            usage,
            color_space: usage.color_space(),
        }
    }

    /// Return the canonical path portion of this key.
    pub fn path(&self) -> &str {
        &self.canonical_path
    }

    /// Return the usage portion of this key.
    pub const fn usage(&self) -> TextureUsage {
        self.usage
    }

    /// Return the color-space portion of this key.
    pub const fn color_space(&self) -> ColorSpace {
        self.color_space
    }
}

/// Fallback classification for an absent texture slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFallback {
    /// A one-pixel white texture.
    White,
    /// A one-pixel tangent-space normal of (0, 0, 1).
    FlatNormal,
    /// A one-pixel black texture.
    Black,
}

/// A resolved texture slot, including the cache identity and fallback policy.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureBinding {
    /// Cache key containing canonical path, usage, and color space.
    pub key: TextureCacheKey,
    /// Explicit slot usage.
    pub usage: TextureUsage,
    /// Fixed color space for the usage.
    pub color_space: ColorSpace,
    /// Loaded CPU texture handle, when the asset exists and decodes.
    pub handle: Option<Handle<CpuTexture>>,
    /// Stable fallback selected when the handle is absent.
    pub fallback: TextureFallback,
}

/// Opaque material values consumed by the later extraction/GPU stages.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMaterial {
    /// Base color multiplier.
    pub base_color: [f32; 4],
    /// Metallic factor.
    pub metallic: f32,
    /// Roughness factor.
    pub roughness: f32,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Occlusion contribution.
    pub occlusion_strength: f32,
    /// Emissive color multiplier.
    pub emissive: [f32; 3],
    /// Emissive intensity.
    pub emissive_intensity: f32,
    /// Resolved albedo handle.
    pub albedo: Option<Handle<CpuTexture>>,
    /// Resolved normal handle.
    pub normal: Option<Handle<CpuTexture>>,
    /// Resolved ORM handle.
    pub orm: Option<Handle<CpuTexture>>,
    /// Resolved emissive handle.
    pub emissive_texture: Option<Handle<CpuTexture>>,
}

/// Complete result of resolving a [`MaterialConfig`].
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialResolution {
    /// Scalar and handle material consumed by the next pipeline stage.
    pub material: ResolvedMaterial,
    /// Legacy unlit marker retained for the flag-off MaterialUniform adapter.
    pub legacy_unlit: bool,
    /// Albedo binding and fallback.
    pub albedo: TextureBinding,
    /// Normal binding and fallback.
    pub normal: TextureBinding,
    /// ORM binding and fallback.
    pub orm: TextureBinding,
    /// Emissive binding and fallback.
    pub emissive: TextureBinding,
    /// Explicit ORM channel mapping.
    pub orm_swizzle: OrmSwizzle,
}

/// Errors that reject a material before it can reach a consumer.
#[derive(Debug, Error, PartialEq)]
pub enum MaterialResolveError {
    /// A scalar contains NaN or infinity.
    #[error("material field '{field}' must be finite")]
    NonFinite {
        /// Field name used in diagnostics.
        field: &'static str,
    },
    /// A scalar is outside its frozen contract range.
    #[error("material field '{field}'={value} is outside [{min}, {max}]")]
    OutOfRange {
        /// Field name used in diagnostics.
        field: &'static str,
        /// Invalid value.
        value: f32,
        /// Inclusive lower bound.
        min: f32,
        /// Inclusive upper bound.
        max: f32,
    },
    /// A texture exists but failed to decode.
    #[error("failed to load {usage:?} texture '{path}': {message}")]
    TextureLoad {
        /// Texture usage.
        usage: TextureUsage,
        /// Canonical texture path.
        path: String,
        /// Decoder or I/O error text.
        message: String,
    },
}

/// Resolves scene material configuration against one project asset root.
#[derive(Debug, Clone)]
pub struct MaterialResolver {
    asset_root: PathBuf,
}

impl MaterialResolver {
    /// Create a resolver rooted at the project/scene asset directory.
    pub fn new(asset_root: impl AsRef<Path>) -> Self {
        Self {
            asset_root: normalize_path(asset_root.as_ref()),
        }
    }

    /// Resolve legacy or extended material configuration into one contract.
    pub fn resolve(
        &self,
        config: &MaterialConfig,
        assets: &mut AssetManager,
    ) -> Result<MaterialResolution, MaterialResolveError> {
        validate_config(config)?;

        let albedo = self.resolve_texture(
            config.albedo_texture.as_deref(),
            TextureUsage::Albedo,
            assets,
        )?;
        let normal = self.resolve_texture(
            config.normal_texture.as_deref(),
            TextureUsage::Normal,
            assets,
        )?;
        let orm = self.resolve_texture(config.orm_texture.as_deref(), TextureUsage::Orm, assets)?;
        let emissive = self.resolve_texture(
            config.emissive_texture.as_deref(),
            TextureUsage::Emissive,
            assets,
        )?;

        Ok(MaterialResolution {
            material: ResolvedMaterial {
                base_color: config.albedo,
                metallic: config.metallic,
                roughness: config.roughness,
                normal_scale: config.normal_scale,
                occlusion_strength: config.occlusion_strength,
                emissive: config.emissive,
                emissive_intensity: config.emissive_intensity,
                albedo: albedo.handle.clone(),
                normal: normal.handle.clone(),
                orm: orm.handle.clone(),
                emissive_texture: emissive.handle.clone(),
            },
            legacy_unlit: config.unlit,
            albedo,
            normal,
            orm,
            emissive,
            orm_swizzle: config.orm_swizzle,
        })
    }

    fn resolve_texture(
        &self,
        raw_path: Option<&str>,
        usage: TextureUsage,
        assets: &mut AssetManager,
    ) -> Result<TextureBinding, MaterialResolveError> {
        let normalized = raw_path
            .map(|path| normalize_path(&self.asset_root.join(path)))
            .unwrap_or_else(|| self.asset_root.join("<fallback>"));
        let path = normalized.to_string_lossy().into_owned();
        let key = TextureCacheKey::new(path.clone(), usage);
        let handle = match raw_path {
            None => None,
            Some(_) if !normalized.is_file() => None,
            Some(_) => Some(assets.load::<CpuTexture>(&normalized).map_err(|error| {
                MaterialResolveError::TextureLoad {
                    usage,
                    path: path.clone(),
                    message: error.to_string(),
                }
            })?),
        };

        Ok(TextureBinding {
            key,
            usage,
            color_space: usage.color_space(),
            handle,
            fallback: usage.fallback(),
        })
    }
}

fn validate_config(config: &MaterialConfig) -> Result<(), MaterialResolveError> {
    for (field, value) in [
        ("normal_scale", config.normal_scale),
        ("occlusion_strength", config.occlusion_strength),
        ("emissive_intensity", config.emissive_intensity),
    ] {
        if !value.is_finite() {
            return Err(MaterialResolveError::NonFinite { field });
        }
    }
    if !(0.0..=2.0).contains(&config.normal_scale) {
        return Err(MaterialResolveError::OutOfRange {
            field: "normal_scale",
            value: config.normal_scale,
            min: 0.0,
            max: 2.0,
        });
    }
    if !(0.0..=1.0).contains(&config.occlusion_strength) {
        return Err(MaterialResolveError::OutOfRange {
            field: "occlusion_strength",
            value: config.occlusion_strength,
            min: 0.0,
            max: 1.0,
        });
    }
    if config.emissive_intensity < 0.0 {
        return Err(MaterialResolveError::OutOfRange {
            field: "emissive_intensity",
            value: config.emissive_intensity,
            min: 0.0,
            max: f32::INFINITY,
        });
    }
    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
#[path = "material_tests.rs"]
mod tests;
