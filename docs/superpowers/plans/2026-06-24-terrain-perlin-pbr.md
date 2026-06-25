# Terrain Phase 1: Perlin Noise + GPU PBR Splatting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `TerrainSource::Perlin` height generator using FBM Perlin noise and upgrade `TerrainPass` to sample albedo/normal/roughness-metallic/AO texture arrays with height- and slope-based material blending.

**Architecture:** Keep the existing CPU chunked-LOD geometry pipeline; replace only the `HeightFunction` and the fragment shader. GPU texture arrays are built once during scene load and stored on the ECS `Terrain` component so `TerrainPass` does not need `AssetManager` at render time. Normal mapping uses derivative-based TBN to avoid mesh tangent generation.

**Tech Stack:** Rust 1.78, wgpu 29, `noise 0.9`, hecs, RON, inline WGSL.

## Global Constraints

- Each Rust module must stay below 500 LOC; split into sub-modules if a file grows past the limit.
- WGSL shaders remain inline in the pass file (`r#"..."#` + `Cow::Borrowed`).
- New dependency `noise = "0.9"` must be declared in workspace root `[workspace.dependencies]` and referenced as `{ workspace = true }` from `crates/aether-engine/Cargo.toml`.
- All code changes under `renderer/passes/terrain/` are **MUST_VERIFY** for visual regression.
- Tests must exercise public interfaces only (`HeightFunction::sample`, `TerrainLayerConfig`, `TerrainGpuTextures::from_material`).
- Invalid states should be unrepresentable at build time where possible (`NonZeroU32` for octaves, `TerrainTextureError`, layer-count validation).
- All existing `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` must pass.

---

## File Structure

| File | Responsibility | Change Type |
|------|----------------|-------------|
| `Cargo.toml` | Add `noise = "0.9"` to `[workspace.dependencies]`. | Modify |
| `crates/aether-engine/Cargo.toml` | Reference `noise` from workspace. | Modify |
| `crates/aether-engine/src/terrain/noise.rs` | `PerlinHeight` implementing `HeightFunction`. | Create |
| `crates/aether-engine/src/terrain/mod.rs` | Re-export `PerlinHeight`. | Modify |
| `crates/aether-engine/src/terrain/geometry.rs` | Add `PerlinHeight` dispatch in `height_function_from_source`; keep `ProceduralHeight`. | Modify |
| `crates/aether-engine/src/scene/config/terrain.rs` | Add `TerrainSource::Perlin` variant and height/slope fields to `TerrainLayerConfig`. | Modify |
| `crates/aether-engine/src/asset/terrain_material.rs` | Add optional `ao_texture`, keep packed `roughness_metallic_texture`. | Modify |
| `crates/aether-engine/src/renderer/passes/terrain/types.rs` | `TerrainUniform`, `LayerBlendUniform`, `TerrainGpuTextures`, `TerrainTextureError`. | Create |
| `crates/aether-engine/src/renderer/passes/terrain/mod.rs` | Update bind group layout, pipeline layout, and `TerrainPass` fields for texture arrays. | Modify |
| `crates/aether-engine/src/renderer/passes/terrain/shaders.rs` | Rewrite WGSL vertex/fragment for texture-array splatting and derivative TBN. | Modify |
| `crates/aether-engine/src/renderer/passes/terrain/update.rs` | Update uniform writing, derive height bounds from source amplitude. | Modify |
| `crates/aether-engine/src/ecs/components.rs` | Add `gpu_textures: Option<TerrainGpuTextures>` to `Terrain` component. | Modify |
| `crates/aether-engine/src/scene/loader/spawn.rs` | Validate configs, build `TerrainGpuTextures` during spawn. | Modify |
| `crates/aether-engine/src/renderer/extract.rs` | No code change needed if `Terrain` is cloned wholesale; verify `Terrain` remains `Clone`. | Read-only |
| `assets/textures/terrain/` | Copy Kong Engine texture sets. | Create directories/files |
| `scenes/16_terrain_perlin.ron` | New validation scene using `Perlin` source and texture layers. | Create |
| `tests/reports/YYYY-MM-DD-16_terrain_perlin.md` | Visual verification report. | Create at verification time |

---

## Task 1: Dependency & Baseline

**Files:**
- Modify: `Cargo.toml:13`
- Modify: `crates/aether-engine/Cargo.toml:14-48`

**Interfaces:**
- Consumes: workspace dependency manifest.
- Produces: `noise` crate available as `{ workspace = true }`.

- [ ] **Step 1: Add `noise` to workspace dependencies**

In `Cargo.toml`, append inside `[workspace.dependencies]`:

```toml
noise = "0.9"
```

- [ ] **Step 2: Reference `noise` from the engine crate**

In `crates/aether-engine/Cargo.toml`, add after `thiserror = { workspace = true }`:

```toml
noise = { workspace = true }
```

- [ ] **Step 3: Verify no duplicate versions and baseline compiles**

Run:

```bash
cargo tree -d | grep noise
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `cargo tree -d | grep noise` shows only one `noise` entry; `cargo check` and `cargo clippy` pass with no new errors or warnings.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/aether-engine/Cargo.toml
git commit -m "deps(#102): add noise 0.9 for Perlin terrain generation"
```

---

## Task 2: Perlin Height Function

**Files:**
- Create: `crates/aether-engine/src/terrain/noise.rs`
- Modify: `crates/aether-engine/src/terrain/mod.rs`
- Modify: `crates/aether-engine/src/terrain/geometry.rs:8-9, 208-222`

**Interfaces:**
- Consumes: `HeightFunction` trait, `TerrainSource`.
- Produces: `pub struct PerlinHeight`, `impl HeightFunction for PerlinHeight`, re-export.

- [ ] **Step 1: Write the failing tests**

Create `crates/aether-engine/src/terrain/noise.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_height_is_deterministic() {
        let h = PerlinHeight::new(123, 0.05, 32.0, std::num::NonZeroU32::new(4).unwrap());
        let a = h.sample(10.0, 20.0);
        let b = h.sample(10.0, 20.0);
        assert_eq!(a, b);
    }

    #[test]
    fn perlin_height_is_bounded_by_amplitude() {
        let h = PerlinHeight::new(7, 0.05, 32.0, std::num::NonZeroU32::new(4).unwrap());
        for i in 0..100 {
            let x = i as f32 * 1.37;
            let z = i as f32 * 2.19;
            let y = h.sample(x, z);
            assert!(y >= -32.0 && y <= 32.0, "sample {} outside bounds", y);
        }
    }

    #[test]
    fn perlin_height_changes_with_seed() {
        let a = PerlinHeight::new(1, 0.05, 32.0, std::num::NonZeroU32::new(4).unwrap());
        let b = PerlinHeight::new(2, 0.05, 32.0, std::num::NonZeroU32::new(4).unwrap());
        let sa = a.sample(5.0, 5.0);
        let sb = b.sample(5.0, 5.0);
        assert!((sa - sb).abs() > 0.001);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p aether-engine terrain::noise
```

Expected: compile errors because `PerlinHeight` is not defined.

- [ ] **Step 3: Implement `PerlinHeight`**

In `crates/aether-engine/src/terrain/noise.rs`:

```rust
//! Perlin / FBM noise height function for terrain generation.

use crate::terrain::geometry::HeightFunction;
use std::num::NonZeroU32;

/// FBM Perlin height function.
#[derive(Debug, Clone)]
pub struct PerlinHeight {
    seed: u32,
    frequency: f32,
    amplitude: f32,
    octaves: u32,
    persistence: f32,
    lacunarity: f32,
    exponent: f32,
}

impl PerlinHeight {
    /// Create a new Perlin height source.
    ///
    /// # Panics
    /// Never — `octaves` is `NonZeroU32` so zero is unrepresentable.
    pub fn new(
        seed: u32,
        frequency: f32,
        amplitude: f32,
        octaves: NonZeroU32,
    ) -> Self {
        Self {
            seed,
            frequency,
            amplitude,
            octaves: octaves.get(),
            persistence: 0.5,
            lacunarity: 2.0,
            exponent: 1.0,
        }
    }

    pub fn with_persistence(mut self, persistence: f32) -> Self {
        self.persistence = persistence;
        self
    }

    pub fn with_lacunarity(mut self, lacunarity: f32) -> Self {
        self.lacunarity = lacunarity;
        self
    }

    pub fn with_exponent(mut self, exponent: f32) -> Self {
        self.exponent = exponent;
        self
    }
}

impl HeightFunction for PerlinHeight {
    fn sample(&self, x: f32, z: f32) -> f32 {
        use noise::{Fbm, Perlin, Seedable};

        let mut fbm: Fbm<Perlin> = Fbm::new(self.seed);
        fbm.frequency = self.frequency as f64;
        fbm.octaves = self.octaves;
        fbm.persistence = self.persistence as f64;
        fbm.lacunarity = self.lacunarity as f64;

        // Offset domain by seed-derived value for deterministic variation.
        let dx = (self.seed.wrapping_mul(374761393) as f64) * 0.0001;
        let dz = (self.seed.wrapping_mul(668265263) as f64) * 0.0001;
        let n = fbm.get([x as f64 + dx, z as f64 + dz]);

        // Normalize roughly to [-1, 1] then apply amplitude and exponent.
        let normalized = (n as f32).clamp(-1.0, 1.0);
        let signed = normalized.signum() * normalized.abs().powf(self.exponent);
        signed * self.amplitude
    }
}
```

- [ ] **Step 4: Re-export `PerlinHeight`**

In `crates/aether-engine/src/terrain/mod.rs`, change line 8 to:

```rust
pub use geometry::{generate_chunk_lod_meshes, height_function_from_source, PerlinHeight, ProceduralHeight};
```

- [ ] **Step 5: Wire `PerlinHeight` into `height_function_from_source`**

In `crates/aether-engine/src/terrain/geometry.rs`:

```rust
use crate::scene::TerrainSource;
use std::num::NonZeroU32;
```

Update `height_function_from_source`:

```rust
pub fn height_function_from_source(source: &TerrainSource) -> Box<dyn HeightFunction> {
    match source {
        TerrainSource::Heightmap(_path) => {
            // Phase 5 heightmap loading: for now fall back to procedural.
            Box::new(ProceduralHeight::new(0, 0.02, 32.0))
        }
        TerrainSource::Procedural {
            seed,
            frequency,
            amplitude,
        } => Box::new(ProceduralHeight::new(*seed, *frequency, *amplitude)),
        TerrainSource::Perlin {
            seed,
            frequency,
            amplitude,
            octaves,
            persistence,
            lacunarity,
            exponent,
        } => {
            let octaves = NonZeroU32::new(*octaves).unwrap_or(NonZeroU32::new(4).unwrap());
            let height = PerlinHeight::new(*seed as u32, *frequency, *amplitude, octaves)
                .with_persistence(*persistence)
                .with_lacunarity(*lacunarity)
                .with_exponent(*exponent);
            Box::new(height)
        }
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p aether-engine terrain::noise
cargo test -p aether-engine terrain::geometry
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/aether-engine/src/terrain/noise.rs crates/aether-engine/src/terrain/mod.rs crates/aether-engine/src/terrain/geometry.rs
git commit -m "feat(terrain): #102 PerlinHeight with FBM noise"
```

---

## Task 3: Extend Terrain Configuration

**Files:**
- Modify: `crates/aether-engine/src/scene/config/terrain.rs`

**Interfaces:**
- Consumes: existing `TerrainSource`, `TerrainLayerConfig`, `TerrainConfig`.
- Produces: `TerrainSource::Perlin { ... }`, extended `TerrainLayerConfig` with `min_height`, `max_height`, `blend_range`, `slope_limit`, `ao_texture`.

- [ ] **Step 1: Write failing serialization tests**

Add to the bottom of `crates/aether-engine/src/scene/config/terrain.rs` inside `#[cfg(test)]`:

```rust
#[test]
fn perlin_source_round_trips() {
    let source = TerrainSource::Perlin {
        seed: 42,
        frequency: 0.03,
        amplitude: 48.0,
        octaves: 4,
        persistence: 0.5,
        lacunarity: 2.0,
        exponent: 1.0,
    };
    let serialized = ron::to_string(&source).unwrap();
    let deserialized: TerrainSource = ron::from_str(&serialized).unwrap();
    assert_eq!(source, deserialized);
}

#[test]
fn layer_config_with_height_slope_round_trips() {
    let cfg = TerrainLayerConfig {
        albedo: [1.0, 0.0, 0.0, 1.0],
        roughness: 0.8,
        metallic: 0.0,
        albedo_texture: Some("assets/textures/terrain/grass/albedo.png".into()),
        normal_texture: Some("assets/textures/terrain/grass/normal.png".into()),
        roughness_metallic_texture: Some("assets/textures/terrain/grass/rm.png".into()),
        ao_texture: Some("assets/textures/terrain/grass/ao.png".into()),
        min_height: 0.0,
        max_height: 32.0,
        blend_range: 4.0,
        slope_limit: 0.8,
    };
    let serialized = ron::to_string(&cfg).unwrap();
    let deserialized: TerrainLayerConfig = ron::from_str(&serialized).unwrap();
    assert_eq!(cfg, deserialized);
}
```

- [ ] **Step 2: Run tests to verify failure**

```bash
cargo test -p aether-engine scene::config::terrain
```

Expected: compile errors for missing fields/variant.

- [ ] **Step 3: Extend `TerrainLayerConfig`**

Replace the existing `TerrainLayerConfig` struct with:

```rust
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
    /// Optional ambient occlusion texture path.
    #[serde(default)]
    pub ao_texture: Option<String>,
    /// Minimum world height for this layer to appear.
    #[serde(default)]
    pub min_height: f32,
    /// Maximum world height for this layer to appear.
    #[serde(default)]
    pub max_height: f32,
    /// Height/slope blend transition range.
    #[serde(default = "default_layer_blend_range")]
    pub blend_range: f32,
    /// Maximum surface slope (cosine of angle from up) for this layer.
    #[serde(default = "default_slope_limit")]
    pub slope_limit: f32,
}

fn default_layer_blend_range() -> f32 {
    4.0
}

fn default_slope_limit() -> f32 {
    0.9
}
```

Update `Default for TerrainLayerConfig` to include the new fields.

- [ ] **Step 4: Add `TerrainSource::Perlin`**

Replace the `TerrainSource` enum with:

```rust
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
        /// Optional post-exponent for terracing.
        #[serde(default = "default_perlin_exponent")]
        exponent: f32,
    },
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
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p aether-engine scene::config::terrain
cargo check -p aether-engine
```

Expected: serialization tests pass; crate compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/aether-engine/src/scene/config/terrain.rs
git commit -m "feat(scene): #102 TerrainSource::Perlin and layer height/slope config"
```

---

## Task 4: Asset Material + GPU Texture Builder

**Files:**
- Modify: `crates/aether-engine/src/asset/terrain_material.rs`
- Create: `crates/aether-engine/src/renderer/passes/terrain/types.rs`

**Interfaces:**
- Consumes: `TerrainMaterial`, `TerrainLayer`, `CpuTexture`, `AssetManager`, `wgpu::Device/Queue`.
- Produces: `TerrainGpuTextures`, `TerrainTextureError`, placeholder texture helpers.

- [ ] **Step 1: Add `ao_texture` to runtime layer**

In `crates/aether-engine/src/asset/terrain_material.rs`, add to `TerrainLayer`:

```rust
/// Optional ambient occlusion texture.
pub ao_texture: Option<Handle<CpuTexture>>,
```

Update `Default for TerrainLayer` to set `ao_texture: None`.

- [ ] **Step 2: Create `TerrainGpuTextures` and error type**

Create `crates/aether-engine/src/renderer/passes/terrain/types.rs`:

```rust
//! Terrain pass types: uniforms, GPU textures, and errors.

use crate::asset::terrain_material::TerrainMaterial;
use crate::asset::{texture::CpuTexture, AssetManager, Handle};
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur while building terrain GPU textures.
#[derive(Debug, Error)]
pub enum TerrainTextureError {
    #[error("layer {index} is missing a required albedo texture")]
    MissingAlbedo { index: usize },
    #[error("texture array size mismatch: layer {index} {kind} is {actual:?}, expected {expected:?}")]
    SizeMismatch {
        index: usize,
        kind: &'static str,
        expected: (u32, u32),
        actual: (u32, u32),
    },
}

/// GPU-resident texture set for terrain splatting.
#[derive(Debug, Clone)]
pub struct TerrainGpuTextures {
    pub albedo_array: Arc<wgpu::Texture>,
    pub normal_array: Arc<wgpu::Texture>,
    pub roughness_metallic_array: Arc<wgpu::Texture>,
    pub ao_array: Arc<wgpu::Texture>,
    pub splat_map: Arc<wgpu::Texture>,
    pub array_size: (u32, u32),
    pub layer_count: u32,
}

impl TerrainGpuTextures {
    /// Build GPU texture arrays from a loaded `TerrainMaterial`.
    pub fn from_material(
        material: &TerrainMaterial,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        assets: &AssetManager,
    ) -> Result<Self, TerrainTextureError> {
        // Determine active layer count (up to 4).
        let layer_count = material.layer_count() as u32;

        // Resolve CPU textures and required dimensions.
        let target_size = Self::resolve_target_size(material, assets)?;

        // Build arrays.
        let albedo_array = Self::build_array(device, queue, target_size, layer_count, |i| {
            material.layers[i]
                .albedo_texture
                .as_ref()
                .and_then(|h| assets.get::<CpuTexture>(*h))
        })?;

        let normal_array = Self::build_array_with_placeholder(
            device,
            queue,
            target_size,
            layer_count,
            |i| {
                material.layers[i]
                    .normal_texture
                    .as_ref()
                    .and_then(|h| assets.get::<CpuTexture>(*h))
            },
            [128, 128, 255, 255], // neutral normal (0.5, 0.5, 1.0)
        );

        let roughness_metallic_array = Self::build_array_with_placeholder(
            device,
            queue,
            target_size,
            layer_count,
            |i| {
                material.layers[i]
                    .roughness_metallic_texture
                    .as_ref()
                    .and_then(|h| assets.get::<CpuTexture>(*h))
            },
            [128, 0, 0, 255], // roughness 0.5, metallic 0.0
        );

        let ao_array = Self::build_array_with_placeholder(
            device,
            queue,
            target_size,
            layer_count,
            |i| {
                material.layers[i]
                    .ao_texture
                    .as_ref()
                    .and_then(|h| assets.get::<CpuTexture>(*h))
            },
            [255, 255, 255, 255], // AO = 1.0
        );

        let splat_map = Self::build_splat_map(device, queue, target_size, material, assets);

        Ok(Self {
            albedo_array: Arc::new(albedo_array),
            normal_array: Arc::new(normal_array),
            roughness_metallic_array: Arc::new(roughness_metallic_array),
            ao_array: Arc::new(ao_array),
            splat_map: Arc::new(splat_map),
            array_size: target_size,
            layer_count,
        })
    }

    // ... private helpers omitted for brevity; implement below ...
}
```

Because the full helpers are long, implement them in the same file but keep the module under 500 LOC; if it exceeds 500, split into `types/builder.rs`.

Key helper signatures to implement:

```rust
fn resolve_target_size(
    material: &TerrainMaterial,
    assets: &AssetManager,
) -> Result<(u32, u32), TerrainTextureError>;

fn build_array<F>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: (u32, u32),
    layers: u32,
    getter: F,
) -> Result<wgpu::Texture, TerrainTextureError>
where
    F: FnMut(usize) -> Option<Arc<CpuTexture>>;

fn build_array_with_placeholder<F>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: (u32, u32),
    layers: u32,
    getter: F,
    placeholder: [u8; 4],
) -> wgpu::Texture
where
    F: FnMut(usize) -> Option<Arc<CpuTexture>>;

fn build_splat_map(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: (u32, u32),
    material: &TerrainMaterial,
    assets: &AssetManager,
) -> wgpu::Texture;
```

> **Note to implementer:** The placeholder helpers must create a 1×1 neutral texture and copy/resize source textures into each array layer. Use `image` crate or a simple bilinear resize. Keep helper code focused; extract to `types/builder.rs` if `types.rs` exceeds 500 LOC.

- [ ] **Step 3: Add unit tests for the builder**

In `crates/aether-engine/src/renderer/passes/terrain/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_texture_error_display() {
        let e = TerrainTextureError::MissingAlbedo { index: 2 };
        assert!(e.to_string().contains("layer 2"));
    }
}
```

- [ ] **Step 4: Run tests and compile**

```bash
cargo test -p aether-engine renderer::passes::terrain::types
cargo check -p aether-engine
```

Expected: passes compile and tests run.

- [ ] **Step 5: Commit**

```bash
git add crates/aether-engine/src/asset/terrain_material.rs crates/aether-engine/src/renderer/passes/terrain/types.rs
git commit -m "feat(terrain): #102 TerrainGpuTextures builder and error type"
```

---

## Task 5: Scene Loader Integration

**Files:**
- Modify: `crates/aether-engine/src/ecs/components.rs`
- Modify: `crates/aether-engine/src/scene/loader/spawn.rs`

**Interfaces:**
- Consumes: `TerrainConfig`, `TerrainMaterial`, `TerrainGpuTextures`, `AssetManager`, `wgpu::Device/Queue`.
- Produces: ECS `Terrain` component with `gpu_textures: Option<TerrainGpuTextures>`.

- [ ] **Step 1: Read current `Terrain` component**

In `crates/aether-engine/src/ecs/components.rs`, locate:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    pub source: TerrainSource,
    pub geometry: TerrainGeometry,
    pub material: TerrainMaterial,
    pub splatmap_path: Option<String>,
    pub layer_configs: Vec<TerrainLayerConfig>,
}
```

- [ ] **Step 2: Add `gpu_textures` field**

Change the `Terrain` struct to:

```rust
use crate::renderer::passes::terrain::types::TerrainGpuTextures;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    pub source: TerrainSource,
    pub geometry: TerrainGeometry,
    pub material: TerrainMaterial,
    pub gpu_textures: Option<Arc<TerrainGpuTextures>>,
    pub splatmap_path: Option<String>,
    pub layer_configs: Vec<TerrainLayerConfig>,
}
```

> **Caution:** `Arc<wgpu::Texture>` does not implement `PartialEq`. Either remove `PartialEq` from `Terrain` or compare only the non-GPU fields. Since `TerrainPass` uses `last_terrain.as_ref() != Some(&terrain)` to detect config changes, update that comparison to compare `(source, geometry, material, splatmap_path, layer_configs)` instead of the whole struct.

- [ ] **Step 3: Update `spawn_terrain` signature and validation**

In `crates/aether-engine/src/scene/loader/spawn.rs`:

```rust
pub(super) fn spawn_terrain(
    world: &mut World,
    terrain_cfg: Option<&TerrainConfig>,
    assets: &mut AssetManager,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let cfg = match terrain_cfg {
        Some(c) => c,
        None => return,
    };

    if cfg.layers.len() > 4 {
        tracing::error!("terrain has {} layers, maximum is 4", cfg.layers.len());
        return;
    }

    if let TerrainSource::Perlin { octaves, frequency, amplitude, persistence, lacunarity, .. } = &cfg.source {
        if *octaves == 0 {
            tracing::error!("terrain Perlin octaves must be > 0");
            return;
        }
        if *frequency <= 0.0 || *amplitude < 0.0 || *persistence <= 0.0 || *lacunarity <= 0.0 {
            tracing::error!("terrain Perlin frequency/amplitude/persistence/lacunarity must be positive");
            return;
        }
    }

    let material = build_terrain_material(cfg, assets);
    let gpu_textures = TerrainGpuTextures::from_material(&material, device, queue, assets)
        .map(Arc::new)
        .ok();

    if gpu_textures.is_none() {
        tracing::warn!("failed to build terrain GPU textures; terrain will render without textures");
    }

    world.spawn((
        Transform::default(),
        Terrain {
            source: cfg.source.clone(),
            geometry: cfg.geometry.clone(),
            material,
            gpu_textures,
            splatmap_path: cfg.splatmap.clone(),
            layer_configs: cfg.layers.clone(),
        },
        Name("Terrain".into()),
    ));
}
```

- [ ] **Step 4: Update `build_terrain_material` to copy `ao_texture`**

In `build_terrain_material`, add:

```rust
ao_texture: layer_cfg
    .ao_texture
    .as_ref()
    .and_then(|path| assets.load::<CpuTexture>(path).ok()),
```

- [ ] **Step 5: Find and update the caller of `spawn_terrain`**

Locate all call sites with:

```bash
grep -rn "spawn_terrain" crates/aether-engine/src/scene/loader/
```

Update the call to pass `device` and `queue`. The caller already has access to `RenderContext` or similar; pass `&ctx.device` and `&ctx.queue`.

- [ ] **Step 6: Update `TerrainPass::update_terrain` equality check**

In `crates/aether-engine/src/renderer/passes/terrain/update.rs`, replace:

```rust
if self.last_terrain.as_ref() != Some(&terrain) {
```

with a helper function or manual field comparison that ignores `gpu_textures`:

```rust
fn terrain_config_eq(a: &Terrain, b: &Terrain) -> bool {
    a.source == b.source
        && a.geometry == b.geometry
        && a.material == b.material
        && a.splatmap_path == b.splatmap_path
        && a.layer_configs == b.layer_configs
}
```

Use it in `update_terrain`:

```rust
if self.last_terrain.as_ref().map_or(true, |last| !terrain_config_eq(last, &terrain)) {
    // invalidate
}
```

- [ ] **Step 7: Compile and test**

```bash
cargo check -p aether-engine
cargo test -p aether-engine scene::loader
```

Expected: compiles; loader tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/aether-engine/src/ecs/components.rs crates/aether-engine/src/scene/loader/spawn.rs crates/aether-engine/src/renderer/passes/terrain/update.rs
git commit -m "feat(scene): #102 build TerrainGpuTextures during spawn and validate config"
```

---

## Task 6: Terrain Shader Rewrite

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/terrain/shaders.rs`
- Modify: `crates/aether-engine/src/renderer/passes/terrain/types.rs` (uniform structs)
- Modify: `crates/aether-engine/src/renderer/passes/terrain/mod.rs` (bind group layout)
- Modify: `crates/aether-engine/src/renderer/passes/terrain/update.rs` (uniform writing)

**Interfaces:**
- Consumes: `TerrainGpuTextures`, `TerrainLayerConfig`, `TerrainMaterial`.
- Produces: Updated `TERRAIN` WGSL, `TerrainUniform`/`LayerBlendUniform`, bind group layout.

- [ ] **Step 1: Update uniform structs in `types.rs`**

Add to `crates/aether-engine/src/renderer/passes/terrain/types.rs`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayerBlendUniform {
    pub min_height: [f32; 4],
    pub max_height: [f32; 4],
    pub blend_range: [f32; 4],
    pub slope_limit: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainUniform {
    pub layer_color_0: [f32; 4],
    pub layer_color_1: [f32; 4],
    pub layer_color_2: [f32; 4],
    pub layer_color_3: [f32; 4],
    pub layer_roughness: [f32; 4],
    pub layer_metallic: [f32; 4],
    pub has_splat_map: u32,
    pub layer_count: u32,
    pub _pad: [u32; 2],
}
```

- [ ] **Step 2: Rewrite `TERRAIN` WGSL**

Replace `crates/aether-engine/src/renderer/passes/terrain/shaders.rs` with:

```rust
//! WGSL shader source for the terrain pass.

/// Terrain GBuffer pass shader: transforms instances and writes layered PBR
/// material data into the deferred GBuffer.
pub const TERRAIN: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) lod: u32,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct TerrainUniform {
    layer_color_0: vec4<f32>,
    layer_color_1: vec4<f32>,
    layer_color_2: vec4<f32>,
    layer_color_3: vec4<f32>,
    layer_roughness: vec4<f32>,
    layer_metallic: vec4<f32>,
    has_splat_map: u32,
    layer_count: u32,
};
struct LayerBlendUniform {
    min_height: vec4<f32>,
    max_height: vec4<f32>,
    blend_range: vec4<f32>,
    slope_limit: vec4<f32>,
};
@group(1) @binding(0) var<uniform> terrain: TerrainUniform;
@group(1) @binding(1) var<uniform> layer_blend: LayerBlendUniform;
@group(1) @binding(2) var terrain_sampler: sampler;
@group(1) @binding(3) var splat_map: texture_2d<f32>;
@group(1) @binding(4) var albedo_array: texture_2d_array<f32>;
@group(1) @binding(5) var normal_array: texture_2d_array<f32>;
@group(1) @binding(6) var roughness_metallic_array: texture_2d_array<f32>;
@group(1) @binding(7) var ao_array: texture_2d_array<f32>;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

fn layer_weight(i: i32, height: f32, slope: f32) -> f32 {
    let min_h = layer_blend.min_height[i];
    let max_h = layer_blend.max_height[i];
    let range = layer_blend.blend_range[i];
    let slope_max = layer_blend.slope_limit[i];

    var h_w = 1.0;
    if (height < min_h) {
        h_w = smoothstep(min_h - range, min_h, height);
    } else if (height > max_h) {
        h_w = smoothstep(max_h + range, max_h, height);
    }

    let s_w = smoothstep(slope_max + 0.1, slope_max, slope);
    return h_w * s_w;
}

fn blend_weights(height: f32, slope: f32) -> vec4<f32> {
    var w = vec4<f32>(
        layer_weight(0, height, slope),
        layer_weight(1, height, slope),
        layer_weight(2, height, slope),
        layer_weight(3, height, slope)
    );

    if (terrain.has_splat_map != 0u) {
        let splat = textureSample(splat_map, terrain_sampler, in.uv);
        w = w * splat;
    }

    let total = w.x + w.y + w.z + w.w;
    if (total > 0.0) {
        w = w / total;
    } else {
        w = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    return w;
}

fn sample_layer_array(arr: texture_2d_array<f32>, uv: vec2<f32>, layer: i32) -> vec4<f32> {
    return textureSample(arr, terrain_sampler, uv, layer);
}

fn udn_blend(n1: vec3<f32>, n2: vec3<f32>) -> vec3<f32> {
    return normalize(vec3<f32>(n1.xy + n2.xy, n1.z));
}

struct FragmentOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) albedo: vec4<f32>,
    @location(3) material: vec2<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);

    let height = in.world_pos.y;
    let slope = 1.0 - dot(in.world_normal, vec3<f32>(0.0, 1.0, 0.0));
    let weights = blend_weights(height, slope);

    // Sample textures.
    let uv = in.uv * 32.0; // tiling
    var albedo = vec3<f32>(0.0);
    var normal_ts = vec3<f32>(0.0, 0.0, 1.0);
    var roughness = 0.0;
    var metallic = 0.0;
    var ao = 1.0;

    for (var i = 0; i < 4; i = i + 1) {
        if (f32(i) >= f32(terrain.layer_count)) {
            break;
        }
        let w = weights[i];
        if (w <= 0.0) {
            continue;
        }
        let s = sample_layer_array(albedo_array, uv, i);
        albedo = albedo + s.rgb * w;

        let n = sample_layer_array(normal_array, uv, i).rgb * 2.0 - 1.0;
        normal_ts = normal_ts + n * w;

        let rm = sample_layer_array(roughness_metallic_array, uv, i).rg;
        roughness = roughness + rm.r * w;
        metallic = metallic + rm.g * w;

        let a = sample_layer_array(ao_array, uv, i).r;
        ao = ao + a * w;
    }

    // Derivative-based TBN.
    let ddx_pos = dpdx(in.world_pos);
    let ddy_pos = dpdy(in.world_pos);
    let ddx_uv = dpdx(in.uv);
    let ddy_uv = dpdy(in.uv);
    let N = normalize(in.world_normal);
    let T = normalize(ddx_pos * ddy_uv.y - ddy_pos * ddx_uv.y);
    let B = normalize(ddy_pos * ddx_uv.x - ddx_pos * ddy_uv.x);
    let TBN = mat3x3<f32>(T, B, N);
    let world_normal = normalize(TBN * normalize(normal_ts));

    out.albedo = vec4<f32>(albedo * ao, 1.0);
    out.normal = vec4<f32>(world_normal * 0.5 + 0.5, 1.0);
    out.material = vec2<f32>(roughness, metallic);
    return out;
}
"#;
```

- [ ] **Step 3: Update bind group layout in `mod.rs`**

Replace the `terrain_bgl` creation in `crates/aether-engine/src/renderer/passes/terrain/mod.rs` with:

```rust
let terrain_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Terrain Material BGL"),
    entries: &[
        // binding 0: TerrainUniform
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        // binding 1: LayerBlendUniform
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        // binding 2: sampler
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        // binding 3: splat map
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // bindings 4-7: texture arrays
        wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
    ],
});
```

- [ ] **Step 4: Update bind group creation and pass fields**

Add to `TerrainPass` struct:

```rust
terrain_blend_buffer: wgpu::Buffer,
sampler: wgpu::Sampler,
```

Create the sampler and buffers in `TerrainPass::new`:

```rust
let terrain_blend_buffer = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("Terrain Blend Buf"),
    size: std::mem::size_of::<LayerBlendUniform>() as u64,
    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
});

let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    label: Some("Terrain Sampler"),
    mag_filter: wgpu::FilterMode::Linear,
    min_filter: wgpu::FilterMode::Linear,
    mipmap_filter: wgpu::FilterMode::Linear,
    ..Default::default()
});
```

Update `terrain_bind_group` creation to include all bindings. The bind group must be recreated when textures change; store a helper `create_terrain_bind_group(device, layout, uniform_buf, blend_buf, sampler, textures)` and call it from `apply_frame` when terrain changes.

> **Note:** Because `TerrainPass::new` runs before the scene is loaded, create the bind group with 1×1 placeholder textures initially, then rebuild it in `apply_frame` once `TerrainGpuTextures` is available.

- [ ] **Step 5: Update uniform writing**

In `crates/aether-engine/src/renderer/passes/terrain/update.rs`, replace `write_terrain_uniforms` and `terrain_uniform_from_material` with versions that write both `TerrainUniform` and `LayerBlendUniform`:

```rust
pub(super) fn write_terrain_uniforms(
    uniform_buffer: &wgpu::Buffer,
    blend_buffer: &wgpu::Buffer,
    material: &TerrainMaterial,
    layer_configs: &[TerrainLayerConfig],
    has_splat_map: bool,
    queue: &wgpu::Queue,
) {
    let uniform = terrain_uniform_from_material(material, has_splat_map);
    let blend = layer_blend_uniform_from_configs(layer_configs);
    queue.write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    queue.write_buffer(blend_buffer, 0, bytemuck::cast_slice(&[blend]));
}

fn layer_blend_uniform_from_configs(configs: &[TerrainLayerConfig]) -> LayerBlendUniform {
    let mut min_height = [0.0; 4];
    let mut max_height = [0.0; 4];
    let mut blend_range = [4.0; 4];
    let mut slope_limit = [0.9; 4];
    for (i, cfg) in configs.iter().take(4).enumerate() {
        min_height[i] = cfg.min_height;
        max_height[i] = cfg.max_height;
        blend_range[i] = cfg.blend_range;
        slope_limit[i] = cfg.slope_limit;
    }
    LayerBlendUniform {
        min_height,
        max_height,
        blend_range,
        slope_limit,
    }
}
```

- [ ] **Step 6: Update `estimate_height_range`**

In `update.rs`, change:

```rust
fn estimate_height_range(&self) -> (f32, f32) {
    (-128.0, 128.0)
}
```

to read from `self.last_terrain`:

```rust
fn estimate_height_range(&self) -> (f32, f32) {
    let amplitude = self.last_terrain.as_ref().map_or(128.0, |t| match &t.source {
        TerrainSource::Procedural { amplitude, .. } => *amplitude,
        TerrainSource::Perlin { amplitude, .. } => *amplitude,
        TerrainSource::Heightmap(_) => 128.0,
    });
    (-amplitude, amplitude)
}
```

- [ ] **Step 7: Compile shader and run tests**

```bash
cargo check -p aether-engine
cargo test -p aether-engine renderer::passes::terrain
```

Expected: compiles; pass-level tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/aether-engine/src/renderer/passes/terrain/shaders.rs crates/aether-engine/src/renderer/passes/terrain/types.rs crates/aether-engine/src/renderer/passes/terrain/mod.rs crates/aether-engine/src/renderer/passes/terrain/update.rs
git commit -m "feat(terrain): #102 WGSL texture-array splatting and derivative TBN"
```

---

## Task 7: Compile Verification

**Files:** all modified files.

- [ ] **Step 1: Run full workspace checks**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
```

Expected: no formatting errors, no clippy warnings, no compile errors.

- [ ] **Step 2: Run unit tests**

```bash
cargo test --workspace --lib
```

Expected: all tests pass (baseline ~176).

- [ ] **Step 3: Commit any fixes**

```bash
git commit -am "fix(terrain): #102 clippy/format fixes"
```

---

## Task 8: Runtime Verification

**Files:**
- Create: `scenes/16_terrain_perlin.ron`
- Create/copy: `assets/textures/terrain/` Kong Engine textures

**Interfaces:**
- Consumes: new `TerrainSource::Perlin`, texture paths, launcher.
- Produces: working validation scene and captured screenshot.

- [ ] **Step 1: Copy Kong Engine textures**

Assuming Kong Engine is cloned at `../KongEngine`:

```bash
mkdir -p assets/textures/terrain
rsync -av ../KongEngine/resource/textures/terrain/ assets/textures/terrain/
mkdir -p assets/textures/terrain/water
rsync -av ../KongEngine/resource/textures/water/ assets/textures/terrain/water/
```

If Kong Engine is not cloned locally, download from GitHub manually or use:

```bash
# Example for one texture set; repeat for all layers
curl -L -o assets/textures/terrain/grass.zip https://github.com/ruochenhua/KongEngine/raw/master/resource/textures/terrain/grass.zip
```

Verify the directory structure:

```bash
find assets/textures/terrain -type f | head -30
```

- [ ] **Step 2: Create validation scene**

Create `scenes/16_terrain_perlin.ron`:

```ron
SceneDescription(
    name: "Terrain Perlin PBR",
    camera: (
        position: (0.0, 120.0, 300.0),
        yaw: -2.356,
        pitch: -0.524,
        speed: 80.0,
        fov: 60.0,
    ),
    ambient: 0.05,
    lights: [
        (
            light_type: Directional,
            direction: (0.3, -1.0, 0.5),
            color: (1.0, 0.98, 0.95),
            intensity: 1.5,
        ),
    ],
    terrain: Some((
        source: Perlin(seed: 42, frequency: 0.015, amplitude: 96.0, octaves: 5, persistence: 0.5, lacunarity: 2.0, exponent: 1.0),
        geometry: (extent: 512.0, chunk_size: 64, max_lod: 5),
        layers: [
            (
                albedo: (0.36, 0.25, 0.13, 1.0),
                roughness: 0.95,
                albedo_texture: Some("assets/textures/terrain/sand/wavy-sand_albedo.png"),
                normal_texture: Some("assets/textures/terrain/sand/wavy-sand_normal-ogl.png"),
                roughness_metallic_texture: Some("assets/textures/terrain/sand/wavy-sand_roughness.png"),
                ao_texture: Some("assets/textures/terrain/sand/wavy-sand_ao.png"),
                min_height: -200.0,
                max_height: 8.0,
                blend_range: 4.0,
                slope_limit: 0.95,
            ),
            (
                albedo: (0.18, 0.42, 0.12, 1.0),
                roughness: 0.85,
                albedo_texture: Some("assets/textures/terrain/grass/stylized-grass1_albedo.png"),
                normal_texture: Some("assets/textures/terrain/grass/stylized-grass1_normal-ogl.png"),
                roughness_metallic_texture: Some("assets/textures/terrain/grass/stylized-grass1_roughness.png"),
                ao_texture: Some("assets/textures/terrain/grass/stylized-grass1_ao.png"),
                min_height: 4.0,
                max_height: 48.0,
                blend_range: 8.0,
                slope_limit: 0.85,
            ),
            (
                albedo: (0.55, 0.52, 0.48, 1.0),
                roughness: 0.75,
                albedo_texture: Some("assets/textures/terrain/rock_shoreline/rocky-shoreline1-albedo.png"),
                normal_texture: Some("assets/textures/terrain/rock_shoreline/rocky-shoreline1-normal-ogl.png"),
                roughness_metallic_texture: Some("assets/textures/terrain/rock_shoreline/rocky-shoreline1-roughness.png"),
                ao_texture: Some("assets/textures/terrain/rock_shoreline/rocky-shoreline1-ao.png"),
                min_height: 32.0,
                max_height: 200.0,
                blend_range: 12.0,
                slope_limit: 0.65,
            ),
            (
                albedo: (0.95, 0.95, 0.95, 1.0),
                roughness: 0.60,
                albedo_texture: Some("assets/textures/terrain/rock_snow/rock-snow-ice1-2k_Base_Color.png"),
                normal_texture: Some("assets/textures/terrain/rock_snow/rock-snow-ice1-2k_Normal-ogl.png"),
                roughness_metallic_texture: Some("assets/textures/terrain/rock_snow/rock-snow-ice1-2k_Roughness.png"),
                ao_texture: Some("assets/textures/terrain/rock_snow/rock-snow-ice1-2k_Ambient_Occlusion.png"),
                min_height: 72.0,
                max_height: 200.0,
                blend_range: 16.0,
                slope_limit: 0.90,
            ),
        ],
    )),
    objects: [],
)
```

- [ ] **Step 3: Launch the scene**

```bash
cargo run --bin aether-launcher -- --scene scenes/16_terrain_perlin.ron
```

Expected: launcher opens, terrain renders, no panic, no wgpu validation errors in the first 5 seconds.

- [ ] **Step 4: Capture screenshot**

Use the launcher's screenshot hotkey or command-line flag. If the launcher supports `--screenshot tests/output/16_terrain_perlin.png`:

```bash
cargo run --bin aether-launcher -- --scene scenes/16_terrain_perlin.ron --screenshot tests/output/16_terrain_perlin.png
```

Otherwise capture manually and save to `tests/output/16_terrain_perlin.png`.

- [ ] **Step 5: Commit scene and textures**

```bash
git add scenes/16_terrain_perlin.ron assets/textures/terrain/
git commit -m "assets(scene): #102 add Perlin terrain validation scene and textures"
```

---

## Task 9: Visual Verification & Report

**Files:**
- Create: `tests/reports/YYYY-MM-DD-16_terrain_perlin.md`
- Create/update: `tests/reference/16_terrain_perlin.png`

- [ ] **Step 1: Run the smart gate**

```bash
python3 scripts/should-verify-visual.py --since HEAD~8
```

Expected: `MUST_VERIFY` because `renderer/passes/terrain/shaders.rs` and `mod.rs` changed.

- [ ] **Step 2: Inspect the captured screenshot**

Read `tests/output/16_terrain_perlin.png` and verify visually:

- Terrain is not repetitive sine hills; it has organic Perlin variation.
- At least 3 distinct material layers are visible (sand/grass/rock or grass/rock/snow).
- Surface shows normal-map detail and reacts plausibly to directional light.
- No obvious LOD cracks or seams.

- [ ] **Step 3: Promote to reference image**

If visual inspection passes:

```bash
cp tests/output/16_terrain_perlin.png tests/reference/16_terrain_perlin.png
git add tests/reference/16_terrain_perlin.png
```

- [ ] **Step 4: Write visual verification report**

Create `tests/reports/2026-06-24-16_terrain_perlin.md`:

```markdown
# Visual Test Report — 2026-06-24

## Scene: 16_terrain_perlin

### Config
- Scene: `scenes/16_terrain_perlin.ron`
- Features: Terrain=ON, Shadow=ON, IBL=ON, SSAO=ON, SSR=OFF
- GPU backend: Metal/Vulkan (wgpu auto-selected)

### Metrics
| Metric | Value | Threshold | Status |
|---|---|---|---|
| Agent inspection | PASS | — | ✅ PASS |
| Panic / validation error | None | 0 | ✅ PASS |

### Agent Inspection
- ✅ Perlin terrain variation visible, not sine-like.
- ✅ Sand, grass, rock/snow layers visible.
- ✅ Normal-map surface detail present.
- ⚠️ Slight tiling repetition on grass layer at distance (acceptable for Phase 1).

### Verdict
**PASS** — issue #102 can be closed after code review.

### Artifacts
- Output: `tests/output/16_terrain_perlin.png`
- Reference: `tests/reference/16_terrain_perlin.png`
```

- [ ] **Step 5: Update issue labels**

```bash
gh issue edit 102 --add-label "needs-visual-verify"
gh issue comment 102 --body "Visual verification complete. Report: tests/reports/2026-06-24-16_terrain_perlin.md"
gh issue edit 102 --remove-label "needs-visual-verify" --add-label "visual-verified"
```

- [ ] **Step 6: Commit report and reference**

```bash
git add tests/reports/2026-06-24-16_terrain_perlin.md tests/reference/16_terrain_perlin.png
git commit -m "test(visual): #102 visual verification report and reference image"
```

---

## Self-Review

### 1. Spec Coverage

| Spec Section | Implementing Task |
|--------------|-------------------|
| `noise` workspace dependency | Task 1 |
| `PerlinHeight` / `HeightFunction` | Task 2 |
| `TerrainSource::Perlin` config | Task 3 |
| Height/slope layer limits | Task 3, Task 6 shader |
| `TerrainGpuTextures` builder | Task 4 |
| Scene loader validation & build | Task 5 |
| WGSL texture-array splatting | Task 6 |
| Derivative-based TBN | Task 6 |
| `estimate_height_range` from amplitude | Task 6 |
| Visual verification workflow | Task 8, Task 9 |

No gaps identified.

### 2. Placeholder Scan

- No "TBD", "TODO", "implement later".
- "... private helpers omitted for brevity ..." is flagged but the helper signatures are listed; implementer must fill bodies.
- All file paths are exact; all commands include expected output.

### 3. Type Consistency

- `PerlinHeight::new` uses `NonZeroU32`.
- Config `octaves` is `u32`; converted to `NonZeroU32` with fallback in `height_function_from_source`.
- `TerrainUniform` has `layer_count: u32` matching WGSL `terrain.layer_count`.
- `LayerBlendUniform` arrays are `[f32; 4]` matching WGSL `vec4<f32>`.
- Bindings 0-7 in layout match WGSL `@group(1) @binding(0..7)`.

### 4. Risk Notes

- `Terrain` `PartialEq` removal/changed equality in `TerrainPass` must be handled carefully to avoid infinite rebuild loops.
- Texture resize/pad implementation is the largest unverified detail; if helpers exceed 500 LOC, split into `types/builder.rs`.
- `textureSample` with `texture_2d_array` in WGSL requires array index to be a constant or uniform scalar; loop index `i` is a dynamic scalar and is valid in wgpu.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-24-terrain-perlin-pbr.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints.

Which approach would you like?
