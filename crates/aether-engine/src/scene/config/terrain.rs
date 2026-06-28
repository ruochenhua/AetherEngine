//! Terrain configuration.

use serde::{Deserialize, Serialize};

/// Terrain configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainConfig {
    /// Height data source.
    pub source: TerrainSource,
    /// Geometry generation strategy.
    #[serde(default)]
    pub geometry: TerrainGeometry,
    /// Optional splat map texture path.
    #[serde(default)]
    pub splatmap: Option<String>,
    /// Material layers for splatting.
    #[serde(default = "default_terrain_layers")]
    pub layers: Vec<TerrainLayerConfig>,
}

fn default_terrain_layers() -> Vec<TerrainLayerConfig> {
    vec![
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
    ]
}

/// Configuration for a single terrain material layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainLayerConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_layer_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness.
    #[serde(default = "default_layer_roughness")]
    pub roughness: f32,
    /// Surface metallic.
    #[serde(default = "default_layer_metallic")]
    pub metallic: f32,
    /// Optional albedo texture path.
    #[serde(default)]
    pub albedo_texture: Option<String>,
    /// Optional normal map path.
    #[serde(default)]
    pub normal_texture: Option<String>,
    /// Optional packed roughness/metallic texture path.
    #[serde(default)]
    pub roughness_metallic_texture: Option<String>,
    /// Per-layer UV scale. A value of 1.0 uses the global albedo tiling rate;
    /// larger values make this layer's texture tile more densely.
    #[serde(default = "default_layer_uv_scale")]
    pub uv_scale: f32,
}

fn default_layer_albedo() -> [f32; 4] {
    [0.5, 0.5, 0.5, 1.0]
}
fn default_layer_roughness() -> f32 {
    0.8
}
fn default_layer_metallic() -> f32 {
    0.0
}
fn default_layer_uv_scale() -> f32 {
    1.0
}

impl Default for TerrainLayerConfig {
    fn default() -> Self {
        Self {
            albedo: default_layer_albedo(),
            roughness: default_layer_roughness(),
            metallic: default_layer_metallic(),
            albedo_texture: None,
            normal_texture: None,
            roughness_metallic_texture: None,
            uv_scale: default_layer_uv_scale(),
        }
    }
}

/// Height data source for terrain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TerrainSource {
    /// Load height from an image file (grayscale heightmap).
    Heightmap(String),
    /// Procedurally generated height field using layered sine waves.
    Procedural {
        /// Random seed.
        seed: u64,
        /// Base noise frequency.
        #[serde(default = "default_noise_frequency")]
        frequency: f32,
        /// Maximum displacement amplitude.
        #[serde(default = "default_noise_amplitude")]
        amplitude: f32,
    },
    /// FBM Perlin noise generated height field.
    Perlin {
        /// Random seed.
        seed: u64,
        /// Base noise frequency.
        #[serde(default = "default_noise_frequency")]
        frequency: f32,
        /// Maximum displacement amplitude.
        #[serde(default = "default_noise_amplitude")]
        amplitude: f32,
        /// Number of FBM octaves.
        #[serde(default = "default_perlin_octaves")]
        octaves: u32,
        /// Amplitude decay per octave.
        #[serde(default = "default_perlin_persistence")]
        persistence: f32,
        /// Frequency multiplier per octave.
        #[serde(default = "default_perlin_lacunarity")]
        lacunarity: f32,
        /// Optional post-exponent for terracing (1.0 = linear).
        #[serde(default = "default_perlin_exponent")]
        exponent: f32,
    },
}

fn default_noise_frequency() -> f32 {
    0.05
}
fn default_noise_amplitude() -> f32 {
    32.0
}
fn default_perlin_octaves() -> u32 {
    4
}
fn default_perlin_persistence() -> f32 {
    0.5
}
fn default_perlin_lacunarity() -> f32 {
    2.0
}
fn default_perlin_exponent() -> f32 {
    1.0
}

/// Geometry generation strategy for terrain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainGeometry {
    /// World-space half-extent from the origin.
    #[serde(default = "default_terrain_extent")]
    pub extent: f32,
    /// Number of vertices along each chunk edge.
    #[serde(default = "default_terrain_chunk_size")]
    pub chunk_size: u32,
    /// Maximum LOD level (0 = single chunk).
    #[serde(default = "default_terrain_max_lod")]
    pub max_lod: u32,
    /// Global albedo texture tiling rate. The number of texture repeats across
    /// the full terrain width (2 * extent).
    #[serde(default = "default_terrain_albedo_tiling")]
    pub albedo_tiling: f32,
}

fn default_terrain_extent() -> f32 {
    256.0
}
fn default_terrain_chunk_size() -> u32 {
    64
}
fn default_terrain_max_lod() -> u32 {
    4
}
fn default_terrain_albedo_tiling() -> f32 {
    64.0
}

impl Default for TerrainGeometry {
    fn default() -> Self {
        Self {
            extent: default_terrain_extent(),
            chunk_size: default_terrain_chunk_size(),
            max_lod: default_terrain_max_lod(),
            albedo_tiling: default_terrain_albedo_tiling(),
        }
    }
}
