//! Canonical material component passed from scene resolution to renderer extraction.

use crate::asset::texture::CpuTexture;
use crate::asset::Handle;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::config::material::{OrmSwizzle, TextureChannel};
use crate::scene::material::{ColorSpace, MaterialResolution, TextureBinding};

/// Canonical resolved material data attached to renderable scene entities.
///
/// `legacy_adapter` keeps old call sites and feature-off extraction compatible
/// while active extraction reads this component and its resolved texture slots.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtractedMaterial {
    /// Full resolver output, including texture usage, color space, and fallback policy.
    pub resolution: Option<MaterialResolution>,
    /// GPU uniform adapter retained for legacy passes and compatibility mode.
    pub legacy_adapter: MaterialUniform,
}

impl ExtractedMaterial {
    /// Preserve the complete resolver result and build its legacy uniform adapter.
    pub fn from_resolution(resolution: &MaterialResolution) -> Self {
        Self {
            resolution: Some(resolution.clone()),
            legacy_adapter: MaterialUniform::from_resolution(resolution),
        }
    }

    /// Wrap a material created by an older runtime/editor path.
    pub fn from_legacy_uniform(material: MaterialUniform) -> Self {
        Self {
            resolution: None,
            legacy_adapter: material,
        }
    }

    /// Return the uniform representation expected by current GPU layouts.
    pub const fn uniform_adapter(&self) -> &MaterialUniform {
        &self.legacy_adapter
    }

    /// Return resolved ORM channel selectors, defaulting old materials to RGB.
    pub fn orm_swizzle(&self) -> OrmSwizzle {
        self.resolution
            .as_ref()
            .map_or_else(OrmSwizzle::default, |value| value.orm_swizzle)
    }

    /// Return stable compact channel selectors for batch identity and GPU packing.
    pub fn orm_swizzle_bits(&self) -> [u8; 3] {
        let swizzle = self.orm_swizzle();
        [
            channel_code(swizzle.ao),
            channel_code(swizzle.roughness),
            channel_code(swizzle.metallic),
        ]
    }

    /// Return the resolved albedo texture, or adapt the legacy handle id.
    pub fn albedo_texture(&self) -> Option<Handle<CpuTexture>> {
        resolved_or_legacy(
            self.resolution.as_ref().map(|value| &value.albedo),
            self.legacy_adapter.albedo_texture_id,
        )
    }

    /// Return the resolved normal texture, or adapt the legacy handle id.
    pub fn normal_texture(&self) -> Option<Handle<CpuTexture>> {
        resolved_or_legacy(
            self.resolution.as_ref().map(|value| &value.normal),
            self.legacy_adapter.normal_texture_id,
        )
    }

    /// Return the resolved ORM texture, or adapt the legacy handle id.
    pub fn orm_texture(&self) -> Option<Handle<CpuTexture>> {
        resolved_or_legacy(
            self.resolution.as_ref().map(|value| &value.orm),
            self.legacy_adapter.orm_texture_id,
        )
    }

    /// Return the resolved emissive texture, or adapt the legacy handle id.
    pub fn emissive_texture(&self) -> Option<Handle<CpuTexture>> {
        resolved_or_legacy(
            self.resolution.as_ref().map(|value| &value.emissive),
            self.legacy_adapter.emissive_texture_id,
        )
    }

    /// Whether the resolved albedo slot should be sampled through sRGB decoding.
    pub fn albedo_is_srgb(&self) -> bool {
        self.resolution
            .as_ref()
            .map_or(true, |value| value.albedo.color_space == ColorSpace::Srgb)
    }

    /// Whether the resolved emissive slot should be sampled through sRGB decoding.
    pub fn emissive_is_srgb(&self) -> bool {
        self.resolution
            .as_ref()
            .map_or(true, |value| value.emissive.color_space == ColorSpace::Srgb)
    }
}

fn resolved_or_legacy(
    binding: Option<&TextureBinding>,
    legacy_id: u64,
) -> Option<Handle<CpuTexture>> {
    match binding {
        Some(binding) => binding.handle,
        None => (legacy_id != 0).then(|| Handle::new(legacy_id)),
    }
}

fn channel_code(channel: TextureChannel) -> u8 {
    match channel {
        TextureChannel::R => 0,
        TextureChannel::G => 1,
        TextureChannel::B => 2,
        TextureChannel::A => 3,
        TextureChannel::Zero => 4,
        TextureChannel::One => 5,
    }
}
