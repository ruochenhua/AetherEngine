//! Terrain splatting material.
//!
//! Defines the data structures used by `TerrainPass` to blend up to four
//! material layers based on a splat map. This module is intentionally
//! GPU-agnostic; GPU resources are created by the renderer.

use crate::asset::{texture::CpuTexture, Handle};

/// A single terrain material layer.
///
/// Each layer corresponds to one splat-map channel. Layers are blended in
/// the terrain fragment shader using the weights stored in the splat map.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainLayer {
    /// Diffuse albedo color used when no albedo texture is present.
    pub albedo: [f32; 4],
    /// Base roughness value.
    pub roughness: f32,
    /// Base metallic value.
    pub metallic: f32,
    /// Optional albedo texture.
    pub albedo_texture: Option<Handle<CpuTexture>>,
    /// Optional normal map.
    pub normal_texture: Option<Handle<CpuTexture>>,
    /// Optional packed roughness/metallic texture.
    pub roughness_metallic_texture: Option<Handle<CpuTexture>>,
}

impl Default for TerrainLayer {
    fn default() -> Self {
        Self {
            albedo: [0.5, 0.5, 0.5, 1.0],
            roughness: 0.8,
            metallic: 0.0,
            albedo_texture: None,
            normal_texture: None,
            roughness_metallic_texture: None,
        }
    }
}

/// Material configuration for a terrain entity.
///
/// A splat map stores the blend weights for four layers in its RGBA channels.
/// When no splat map is provided, layer 0 is used everywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainMaterial {
    /// Handle to the splat map texture (RGBA channels = layer weights).
    pub splat_map: Option<Handle<CpuTexture>>,
    /// Fixed array of four material layers.
    pub layers: [TerrainLayer; 4],
}

impl TerrainMaterial {
    /// Create a default terrain material with four identical grey layers.
    pub fn default_layers() -> Self {
        Self {
            splat_map: None,
            layers: [
                TerrainLayer::default(),
                TerrainLayer::default(),
                TerrainLayer::default(),
                TerrainLayer::default(),
            ],
        }
    }

    /// Number of layers. Always four for this implementation.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

impl Default for TerrainMaterial {
    fn default() -> Self {
        Self::default_layers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_material_default_has_four_layers() {
        let material = TerrainMaterial::default();
        assert_eq!(material.layer_count(), 4);
    }

    #[test]
    fn terrain_material_layers_are_indexed() {
        let mut material = TerrainMaterial::default();
        material.layers[0].albedo = [1.0, 0.0, 0.0, 1.0];
        material.layers[1].albedo = [0.0, 1.0, 0.0, 1.0];
        material.layers[2].albedo = [0.0, 0.0, 1.0, 1.0];
        material.layers[3].albedo = [1.0, 1.0, 0.0, 1.0];

        assert_eq!(material.layers[0].albedo, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(material.layers[1].albedo, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(material.layers[2].albedo, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(material.layers[3].albedo, [1.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn terrain_material_layer_defaults() {
        let layer = TerrainLayer::default();
        assert_eq!(layer.albedo, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(layer.roughness, 0.8);
        assert_eq!(layer.metallic, 0.0);
        assert!(layer.albedo_texture.is_none());
        assert!(layer.normal_texture.is_none());
        assert!(layer.roughness_metallic_texture.is_none());
    }
}
