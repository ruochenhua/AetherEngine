# Volumetric Cloud Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace single-layer FBM cloud with Nubis-style multi-noise volumetric clouds (Worley + Perlin-Worley + Curl + Weather) with Henyey-Greenstein lighting and 3 quality presets.

**Architecture:** 4 CPU-generated noise textures feed a rewritten WGSL ray-marcher. Beer's law extinction + double-lobe HG phase for silver-lining. All in `passes/volumetric_cloud/` + `clouds/`. `CloudColor` output unchanged.

**Tech Stack:** Rust + wgpu + WGSL

**Spec:** `docs/superpowers/specs/2026-06-29-volumetric-cloud-redesign.md`

## Global Constraints

- Module size < 500 LOC per file
- Shaders inline as `r#"..."#` raw strings + `Cow::Borrowed`
- No external textures — all noise generated at init via CPU
- CloudColor Rgba16Float output format unchanged
- Pass signature unchanged (read GDepth, write CloudColor)
- Existing tests must pass at every commit

---

### Task 1: Worley + Perlin-Worley noise generators

**Files:**
- Create: `crates/aether-engine/src/renderer/clouds/worley.rs`
- Create: `crates/aether-engine/src/renderer/clouds/perlin_worley.rs`
- Modify: `crates/aether-engine/src/renderer/clouds/mod.rs`

**Interfaces:**
- Consumes: nothing (pure math, no GPU)
- Produces:
  - `pub fn worley_noise_3d(size: u32) -> Vec<u8>` — R8Unorm, domain-warped cellular
  - `pub fn perlin_worley_noise_3d(size: u32) -> Vec<u8>` — R8Unorm, Perlin × Worley product

- [ ] **Step 1: Write failing test for worley_noise_3d**

In `worley.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worley_output_has_correct_size() {
        let data = worley_noise_3d(32);
        assert_eq!(data.len(), 32 * 32 * 32);
    }

    #[test]
    fn worley_output_in_u8_range() {
        let data = worley_noise_3d(16);
        for &v in &data {
            assert!(v <= 255u8, "value out of u8 range");
        }
    }

    #[test]
    fn worley_is_deterministic() {
        let a = worley_noise_3d(16);
        let b = worley_noise_3d(16);
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Run test, expect FAIL**

```bash
cd .worktrees/issue-xxx && cargo test --lib clouds::worley
```
Expected: compilation error (module not found / function not defined)

- [ ] **Step 3: Implement worley_noise_3d**

In `worley.rs`:

```rust
//! 3D Cellular / Worley noise for cloud shapes.
//!
//! Uses a 3D grid of feature points with random jitter.
//! At each voxel, computes distance to the 3 nearest feature points
//! and returns F2 - F1 (edge-like cellular pattern) mapped to [0, 1].

use glam::{IVec3, Vec3};

/// Generate a 3D R8Unorm Worley noise texture.
/// `size` must be a power of 2 for clean wrapping.
pub fn worley_noise_3d(size: u32) -> Vec<u8> {
    let cell_count = 8u32; // 8x8x8 feature-point grid
    let cell_size = size as f32 / cell_count as f32;
    let mut data = vec![0u8; (size * size * size) as usize];

    // Pre-generate feature points: cell_count³ points, each jittered within its cell
    let mut features = Vec::with_capacity((cell_count * cell_count * cell_count) as usize);
    for z in 0..cell_count {
        for y in 0..cell_count {
            for x in 0..cell_count {
                let seed = IVec3::new(x as i32, y as i32, z as i32);
                let jitter = hash3_jitter(seed);
                features.push(Vec3::new(
                    (x as f32 + jitter.x) * cell_size,
                    (y as f32 + jitter.y) * cell_size,
                    (z as f32 + jitter.z) * cell_size,
                ));
            }
        }
    }

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32);

                // Find 2 nearest feature points (with wrapping)
                let mut d0 = f32::MAX;
                let mut d1 = f32::MAX;

                // Search 3x3x3 neighbor cells for wrapping support
                for dz in -1i32..=1i32 {
                    for dy in -1i32..=1i32 {
                        for dx in -1i32..=1i32 {
                            let cx = ((x as f32 / cell_size) as i32 + dx)
                                .rem_euclid(cell_count as i32) as u32;
                            let cy = ((y as f32 / cell_size) as i32 + dy)
                                .rem_euclid(cell_count as i32) as u32;
                            let cz = ((z as f32 / cell_size) as i32 + dz)
                                .rem_euclid(cell_count as i32) as u32;

                            let idx = (cz * cell_count * cell_count + cy * cell_count + cx) as usize;
                            let fp = features[idx];

                            // Toroidal wrapping distance
                            let mut diff = p - fp;
                            diff.x = wrap_diff(diff.x, size as f32);
                            diff.y = wrap_diff(diff.y, size as f32);
                            diff.z = wrap_diff(diff.z, size as f32);
                            let d = diff.length();

                            if d < d0 {
                                d1 = d0;
                                d0 = d;
                            } else if d < d1 {
                                d1 = d;
                            }
                        }
                    }
                }

                // F2 - F1 edge distance (creates cellular boundaries)
                let value = (d1 - d0).clamp(0.0, cell_size);
                let normalized = value / cell_size;
                let idx = (z * size * size + y * size + x) as usize;
                data[idx] = (normalized * 255.0) as u8;
            }
        }
    }

    data
}

/// Deterministic jitter in [0.0, 1.0) per cell.
fn hash3_jitter(cell: IVec3) -> Vec3 {
    let h = |n: i32| -> f32 {
        let mut h = (n.wrapping_mul(374761393) ^ (n >> 13)).wrapping_mul(1274126177);
        h = h ^ (h >> 16);
        h as f32 / u32::MAX as f32
    };
    Vec3::new(h(cell.x), h(cell.y), h(cell.z))
}

/// Wrap a signed delta for toroidal distance.
fn wrap_diff(d: f32, size: f32) -> f32 {
    if d > size * 0.5 { d - size } else if d < -size * 0.5 { d + size } else { d }
}
```

- [ ] **Step 4: Run tests, expect PASS**

```bash
cargo test --lib clouds::worley
```

Expected: 3 tests pass

- [ ] **Step 5: Write failing test for perlin_worley_noise_3d**

In `perlin_worley.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_worley_output_has_correct_size() {
        let data = perlin_worley_noise_3d(32);
        assert_eq!(data.len(), 32 * 32 * 32);
    }

    #[test]
    fn perlin_worley_output_in_u8_range() {
        let data = perlin_worley_noise_3d(16);
        for &v in &data {
            assert!(v <= 255u8);
        }
    }

    #[test]
    fn perlin_worley_is_deterministic() {
        let a = perlin_worley_noise_3d(16);
        let b = perlin_worley_noise_3d(16);
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 6: Implement perlin_worley_noise_3d**

In `perlin_worley.rs`:

```rust
//! Perlin-Worley noise blend — re-maps base Worley with high-frequency Perlin detail.
//!
//! Output = Worley * detail_weight + Perlin * (1 - detail_weight)
//! where detail_weight = 0.7, creating eroded cloud edges with internal structure.

use glam::Vec3;

/// Generate a 3D R8Unorm Perlin-Worley blend noise texture.
pub fn perlin_worley_noise_3d(size: u32) -> Vec<u8> {
    let detail_weight: f32 = 0.7;
    let mut data = vec![0u8; (size * size * size) as usize];

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32) / size as f32;

                // Low-freq Perlin for base structure (2 octaves)
                let perlin_low = fbm_perlin(p * 4.0, 2, 2.0, 0.5);
                // High-freq Perlin for detail (3 octaves)
                let perlin_high = fbm_perlin(p * 16.0, 3, 2.0, 0.5);

                // Remap: subtract coverage threshold, amplify detail
                let base = perlin_low.clamp(0.0, 1.0);
                let detail = perlin_high.clamp(0.0, 1.0);

                let value = base * detail_weight + detail * (1.0 - detail_weight);
                let idx = (z * size * size + y * size + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    data
}

/// Simple FBM (Fractal Brownian Motion) using 3D Perlin-like value noise.
fn fbm_perlin(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_3d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

/// Trilinearly interpolated value noise (same algorithm as clouds/noise.rs).
fn value_noise_3d(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;

    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let u = f.x * f.x * (3.0 - 2.0 * f.x);
    let v = f.y * f.y * (3.0 - 2.0 * f.y);
    let w = f.z * f.z * (3.0 - 2.0 * f.z);

    let hash = |x: i32, y: i32, z: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ z.wrapping_mul(2086444801);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        n as f32 / u32::MAX as f32
    };

    let c000 = hash(ix, iy, iz);
    let c100 = hash(ix + 1, iy, iz);
    let c010 = hash(ix, iy + 1, iz);
    let c110 = hash(ix + 1, iy + 1, iz);
    let c001 = hash(ix, iy, iz + 1);
    let c101 = hash(ix + 1, iy, iz + 1);
    let c011 = hash(ix, iy + 1, iz + 1);
    let c111 = hash(ix + 1, iy + 1, iz + 1);

    let c00 = c000 * (1.0 - u) + c100 * u;
    let c01 = c001 * (1.0 - u) + c101 * u;
    let c10 = c010 * (1.0 - u) + c110 * u;
    let c11 = c011 * (1.0 - u) + c111 * u;

    let c0 = c00 * (1.0 - v) + c10 * v;
    let c1 = c01 * (1.0 - v) + c11 * v;

    c0 * (1.0 - w) + c1 * w
}
```

- [ ] **Step 7: Run tests, expect PASS**

```bash
cargo test --lib clouds::perlin_worley
```

Expected: 3 tests pass

- [ ] **Step 8: Wire pub mod in clouds/mod.rs**

Edit `clouds/mod.rs`:

```rust
pub mod noise;
pub mod worley;
pub mod perlin_worley;
```

- [ ] **Step 9: Full test run**

```bash
cargo test --workspace --lib
```

Expected: all existing tests + 6 new tests pass

- [ ] **Step 10: Commit**

```bash
git add crates/aether-engine/src/renderer/clouds/
git commit -m "feat(cloud): Worley + Perlin-Worley noise generators"
```

---

### Task 2: Curl noise generator

**Files:**
- Create: `crates/aether-engine/src/renderer/clouds/curl.rs`
- Modify: `crates/aether-engine/src/renderer/clouds/mod.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `pub fn curl_noise_3d(size: u32) -> Vec<[i8; 2]>` — RG8Snorm as `[i8; 2]` pairs (x, y curl components)
    The texture format is `wgpu::TextureFormat::Rg8Snorm` so values map to [-128, 127]

- [ ] **Step 1: Write failing test for curl_noise_3d**

In `curl.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_output_has_correct_size() {
        let data = curl_noise_3d(16);
        assert_eq!(data.len(), (16 * 16 * 16) as usize);
    }

    #[test]
    fn curl_values_in_i8_range() {
        let data = curl_noise_3d(16);
        for &[x, y] in &data {
            assert!(x >= -128 && x <= 127);
            assert!(y >= -128 && y <= 127);
        }
    }

    #[test]
    fn curl_is_deterministic() {
        let a = curl_noise_3d(16);
        let b = curl_noise_3d(16);
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Implement curl_noise_3d**

In `curl.rs`:

```rust
//! Curl noise for volumetric cloud displacement.
//!
//! Curl of a 3D Perlin-like potential field.
//! Output is RG8Snorm: 2D curl vector (perpendicular to gradient) per voxel.

use glam::Vec3;

/// Generate a 3D curl noise field. Each voxel gets a 2D curl vector
/// (x-component of curl, y-component of curl) stored as signed bytes.
///
/// The shader uses these to warp the sampling position of the density
/// textures, creating wispy, swirling cloud edges.
pub fn curl_noise_3d(size: u32) -> Vec<[i8; 2]> {
    let mut data = Vec::with_capacity((size * size * size) as usize);

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let scale = 0.05; // large-scale curl features
                let pos = Vec3::new(x as f32, y as f32, z as f32) * scale;

                // Finite-difference curl of potential field
                let eps = 0.01;
                let pot = |p: Vec3| -> f32 { potential(p) };

                let dp_dy = (pot(pos + Vec3::Y * eps) - pot(pos - Vec3::Y * eps)) / (2.0 * eps);
                let dp_dz = (pot(pos + Vec3::Z * eps) - pot(pos - Vec3::Z * eps)) / (2.0 * eps);
                let dp_dx = (pot(pos + Vec3::X * eps) - pot(pos - Vec3::X * eps)) / (2.0 * eps);

                // curl = (dp/dy - dp/dz, dp/dz - dp/dx, dp/dx - dp/dy) → keep x, y
                let curl_x = dp_dy - dp_dz;
                let curl_y = dp_dz - dp_dx;

                // Map [-1, 1] to [-128, 127]
                let ix = (curl_x.clamp(-1.0, 1.0) * 127.0) as i8;
                let iy = (curl_y.clamp(-1.0, 1.0) * 127.0) as i8;
                data.push([ix, iy]);
            }
        }
    }

    data
}

fn potential(p: Vec3) -> f32 {
    fbm_perlin_3d(p, 3, 2.0, 0.5)
}

fn fbm_perlin_3d(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_3d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

fn value_noise_3d(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let u = f.x * f.x * (3.0 - 2.0 * f.x);
    let v = f.y * f.y * (3.0 - 2.0 * f.y);
    let w = f.z * f.z * (3.0 - 2.0 * f.z);

    let hash = |x: i32, y: i32, z: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ z.wrapping_mul(2086444801);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        n as f32 / u32::MAX as f32
    };

    let c000 = hash(ix, iy, iz);
    let c100 = hash(ix + 1, iy, iz);
    let c010 = hash(ix, iy + 1, iz);
    let c110 = hash(ix + 1, iy + 1, iz);
    let c001 = hash(ix, iy, iz + 1);
    let c101 = hash(ix + 1, iy, iz + 1);
    let c011 = hash(ix, iy + 1, iz + 1);
    let c111 = hash(ix + 1, iy + 1, iz + 1);

    let c00 = c000 + (c100 - c000) * u;
    let c01 = c001 + (c101 - c001) * u;
    let c10 = c010 + (c110 - c010) * u;
    let c11 = c011 + (c111 - c011) * u;

    let c0 = c00 + (c10 - c00) * v;
    let c1 = c01 + (c11 - c01) * v;

    c0 + (c1 - c0) * w
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib clouds::curl
```

Expected: 3 tests pass

- [ ] **Step 4: Update clouds/mod.rs**

Edit `clouds/mod.rs`:

```rust
pub mod curl;
pub mod noise;
pub mod perlin_worley;
pub mod worley;
```

- [ ] **Step 5: Full test run + commit**

```bash
cargo test --workspace --lib
git add crates/aether-engine/src/renderer/clouds/
git commit -m "feat(cloud): curl noise generator for cloud displacement"
```

---

### Task 3: Weather map + CloudQuality config

**Files:**
- Create: `crates/aether-engine/src/renderer/clouds/weather.rs`
- Modify: `crates/aether-engine/src/renderer/clouds/mod.rs`
- Modify: `crates/aether-engine/src/scene/config/clouds.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `pub fn generate_weather_map_2d(size: u32) -> Vec<u8>` — R8Unorm 2D coverage mask
  - `CloudQuality` enum: `Low`, `Medium`, `High`
  - `CloudConfig { quality: CloudQuality }`

- [ ] **Step 1: Add CloudQuality to CloudConfig**

In `scene/config/clouds.rs`:

```rust
// Add above the struct definition:
/// Volumetric cloud quality preset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudQuality {
    Low,
    Medium,
    High,
}

impl Default for CloudQuality {
    fn default() -> Self {
        Self::Medium
    }
}

// Add field to CloudConfig struct:
    /// Quality preset controlling noise resolution and step counts.
    #[serde(default)]
    pub quality: CloudQuality,

// Add to CloudConfig::default():
            quality: CloudQuality::default(),
```

- [ ] **Step 2: Write failing test for weather map**

In `weather.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_map_has_correct_size() {
        let data = generate_weather_map_2d(32);
        assert_eq!(data.len(), 32 * 32);
    }

    #[test]
    fn weather_map_values_in_range() {
        let data = generate_weather_map_2d(16);
        for &v in &data {
            assert!(v <= 255u8);
        }
    }
}
```

- [ ] **Step 3: Implement weather map generator**

In `weather.rs`:

```rust
//! 2D Weather map — cloud coverage distribution.
//!
//! A low-resolution coverage mask that determines where clouds should
//! form (high values) vs clear sky (low values).

use glam::Vec2;

/// Generate a 2D R8Unorm weather map with natural-looking coverage patterns.
pub fn generate_weather_map_2d(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size) as usize];

    // Perlin-like FBM creates organic coverage regions
    for y in 0..size {
        for x in 0..size {
            let pos = Vec2::new(x as f32 / size as f32, y as f32 / size as f32);
            let value = fbm_2d(pos * 4.0, 3, 2.0, 0.5);
            let idx = (y * size + x) as usize;
            data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    data
}

fn fbm_2d(p: Vec2, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_2d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

fn value_noise_2d(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;

    let u = f.x * f.x * (3.0 - 2.0 * f.x);
    let v = f.y * f.y * (3.0 - 2.0 * f.y);

    let hash = |x: i32, y: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        n as f32 / u32::MAX as f32
    };

    let n00 = hash(ix, iy);
    let n10 = hash(ix + 1, iy);
    let n01 = hash(ix, iy + 1);
    let n11 = hash(ix + 1, iy + 1);

    let nx0 = n00 + (n10 - n00) * u;
    let nx1 = n01 + (n11 - n01) * u;

    nx0 + (nx1 - nx0) * v
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib clouds::weather
```

Expected: 2 tests pass

- [ ] **Step 5: Update clouds/mod.rs**

```rust
pub mod curl;
pub mod noise;
pub mod perlin_worley;
pub mod weather;
pub mod worley;
```

- [ ] **Step 6: Full test run + commit**

```bash
cargo test --workspace --lib
git add crates/aether-engine/src/renderer/clouds/ crates/aether-engine/src/scene/config/
git commit -m "feat(cloud): weather map + CloudQuality enum + config field"
```

---

### Task 4: Multi-noise upload + bind group in VolumetricCloudPass

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/types.rs`
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs`
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/execute.rs`

**Interfaces:**
- Consumes: `worley_noise_3d`, `perlin_worley_noise_3d`, `curl_noise_3d`, `generate_weather_map_2d`
- Produces:
  - `CloudUniform` extended with quality params (step counts, phase constants)
  - New bind group 1: low-freq Worley + high-freq Perlin-Worley + Curl noise + Weather map + samplers
  - `VolumetricCloudPass` generates all noise textures in `new()`
  - Old shader still runs (no render change yet, just pipeline compiles with new BGL)

- [ ] **Step 1: Extend CloudUniform with quality fields**

In `types.rs`, add to `CloudUniform`:

```rust
// Add after wind_time field:
    /// Quality-dependent parameters: x = primary_steps, y = shadow_steps, z = g_forward, w = g_back.
    pub quality_params: glam::Vec4,
    /// Cloud color gradient: xyz = low_altitude_color, w unused.
    pub cloud_color_low: glam::Vec4,
    /// Cloud color gradient: xyz = high_altitude_color, w unused.
    pub cloud_color_high: glam::Vec4,

// Update Default:
            quality_params: glam::Vec4::new(64.0, 6.0, 0.85, 0.3),
            cloud_color_low: glam::Vec4::new(1.0, 0.98, 0.95, 0.0),
            cloud_color_high: glam::Vec4::new(0.85, 0.90, 0.95, 0.0),
```

- [ ] **Step 2: Generate all noise textures in new_without_upload()**

In `pipeline.rs` (or `mod.rs`), in `new_without_upload()`:

Replace the single `generate_noise_data(NOISE_SIZE)` with multiple noise textures.

Add to `VolumetricCloudPass` struct:

```rust
    // Multi-noise textures
    worley_texture: wgpu::Texture,
    worley_view: wgpu::TextureView,
    perlin_worley_texture: wgpu::Texture,
    perlin_worley_view: wgpu::TextureView,
    curl_texture: wgpu::Texture,
    curl_view: wgpu::TextureView,
    weather_texture: wgpu::Texture,
    weather_view: wgpu::TextureView,
    noise_sampler_3d: wgpu::Sampler,
    noise_sampler_2d: wgpu::Sampler,
```

Generate in `new_without_upload()`:

```rust
let worley_size: u32 = 128; // TODO: vary by quality later
let perlin_worley_size: u32 = 128;
let curl_size: u32 = 16;
let weather_size: u32 = 64;

let worley_data = crate::renderer::clouds::worley::worley_noise_3d(worley_size);
let perlin_worley_data = crate::renderer::clouds::perlin_worley::perlin_worley_noise_3d(perlin_worley_size);
let curl_data = crate::renderer::clouds::curl::curl_noise_3d(curl_size);
let weather_data = crate::renderer::clouds::weather::generate_weather_map_2d(weather_size);

// Create textures (create_texture_3d helper, create_texture_2d helper)…
// Upload with queue if available, or defer to apply_frame…
```

Note: for the `new_without_upload` variant (used by `init()`), textures are created but not uploaded. Upload happens in `apply_frame` like the current noise texture. For the `new(device, queue)` variant, upload immediately.

- [ ] **Step 3: Add new bind group (index 2) for noise textures**

Create `noise_bind_group_layout` with 5 entries:
- @binding(0): 3D texture (worley_tex)
- @binding(1): 3D texture (perlin_worley_tex)
- @binding(2): 3D texture (curl_tex)
- @binding(3): 2D texture (weather_tex)
- @binding(4): sampler (3D noise sampler)

Add bind group 2 to pipeline layout (`bind_group_layouts: &[0, 1, 2]`).

Stub declare in WGSL (keep old `fs_main` logic, just add unused declarations):

```wgsl
@group(2) @binding(0) var worley_tex: texture_3d<f32>;
@group(2) @binding(1) var perlin_worley_tex: texture_3d<f32>;
@group(2) @binding(2) var curl_tex: texture_3d<f32>;
@group(2) @binding(3) var weather_tex: texture_2d<f32>;
@group(2) @binding(4) var noise_sampler: sampler;
```

- [ ] **Step 4: Update execute() to bind group 2**

In `execute.rs`, add after `pass.set_bind_group(1, texture_bg, &[])`:

```rust
pass.set_bind_group(2, &self.noise_bind_group, &[]);
```

- [ ] **Step 5: Update tests to verify multi-noise**

Extend existing test `cloud_noise_texture_has_expected_dimensions` to check all noise textures:

```rust
assert_eq!(pass.worley_texture.width(), 128);
assert_eq!(pass.worley_texture.depth_or_array_layers(), 128);
assert_eq!(pass.perlin_worley_texture.width(), 128);
assert_eq!(pass.weather_texture.width(), 64);
```

- [ ] **Step 6: Full test run**

```bash
cargo test --workspace --lib
```

Expected: all tests pass (old shader still renders)

- [ ] **Step 7: Commit**

```bash
git add crates/aether-engine/src/renderer/passes/volumetric_cloud/ crates/aether-engine/src/renderer/clouds/
git commit -m "feat(cloud): multi-noise texture upload + bind group 2 in VolumetricCloudPass"
```

---

### Task 5: New WGSL shader with HG phase + self-shadowing

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs` (shader only)

**Interfaces:**
- Consumes: multi-noise bind group 2, extended `CloudUniform`
- Produces: identical `CloudColor` Rgba16Float output

- [ ] **Step 1: Rewrite shader source**

Replace the entire `shader_source` in `pipeline.rs` `new_without_upload()` with:

```wgsl
struct CloudUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    cloud_bounds: vec4<f32>,
    wind_time: vec4<f32>,
    quality_params: vec4<f32>,
    cloud_color_low: vec4<f32>,
    cloud_color_high: vec4<f32>,
};

@group(0) @binding(0) var<uniform> clouds: CloudUniform;
@group(1) @binding(0) var depth_tex: texture_depth_2d;
@group(1) @binding(1) var noise_tex: texture_3d<f32>; // legacy, unused in new path
@group(1) @binding(2) var depth_sampler: sampler;     // unused
@group(2) @binding(0) var worley_tex: texture_3d<f32>;
@group(2) @binding(1) var perlin_worley_tex: texture_3d<f32>;
@group(2) @binding(2) var curl_tex: texture_3d<f32>;
@group(2) @binding(3) var weather_tex: texture_2d<f32>;
@group(2) @binding(4) var noise_sampler: sampler;

const PI: f32 = 3.14159265359;

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    return (1.0 - gg) / (4.0 * PI * pow(1.0 + gg - 2.0 * g * cos_theta, 1.5));
}

fn beer_transmittance(optical_depth: f32) -> f32 {
    return exp(-optical_depth);
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth_tex, 0));
    let uv = frag_coord.xy / dims;
    let coord = vec2<i32>(frag_coord.xy);

    let depth = textureLoad(depth_tex, coord, 0);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = clouds.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    let ray_dir = normalize(world_pos - clouds.camera_pos.xyz);
    let bounds = clouds.cloud_bounds;
    let min_y = bounds.x;
    let max_y = bounds.y;

    // --- Slab entry/exit ---
    if (abs(ray_dir.y) < 0.0001) {
        return vec4<f32>(0.0);
    }

    let t_min = (min_y - clouds.camera_pos.y) / ray_dir.y;
    let t_max = (max_y - clouds.camera_pos.y) / ray_dir.y;
    if (t_max < 0.0) {
        return vec4<f32>(0.0);
    }

    var t_enter = max(t_min, 0.0);
    var t_exit = t_max;
    if (t_enter > t_exit) {
        return vec4<f32>(0.0);
    }

    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    t_exit = min(t_exit, geo_dist);
    if (t_enter >= t_exit) {
        return vec4<f32>(0.0);
    }

    // --- Quality params ---
    let primary_steps = clouds.quality_params.x;
    let shadow_steps = clouds.quality_params.y;
    let g_forward = clouds.quality_params.z;
    let g_back = clouds.quality_params.w;
    let coverage = bounds.z;
    let density_scale = bounds.w;

    let sun_dir = normalize(clouds.sun_direction.xyz);
    let wind = clouds.wind_time.xyz * clouds.wind_time.w;
    let dt = (t_exit - t_enter) / primary_steps;

    var transmittance = 1.0;
    var light_energy = 0.0;

    for (var i: f32 = 0.0; i < primary_steps; i += 1.0) {
        let t = t_enter + (i + 0.5) * dt;
        let pos = clouds.camera_pos.xyz + ray_dir * t;

        // Sample weather map for coverage
        let weather_uv = pos.xz * 0.002;
        let weather_val = textureSampleLevel(weather_tex, noise_sampler, weather_uv, 0.0).r;

        // Height-based blend (0 at bottom, 1 at top)
        let height_norm = (pos.y - min_y) / (max_y - min_y);

        // Multi-noise density sampling
        let noise_pos = pos * 0.008 + wind * 0.01;
        let curl_sample = textureSample(curl_tex, noise_sampler, noise_pos * 0.02).rg;
        let warp = vec3<f32>(curl_sample.r * 2.0 - 1.0, 0.0, curl_sample.g * 2.0 - 1.0);
        let warped_pos = noise_pos + warp * 4.0;

        let worley_v = textureSample(worley_tex, noise_sampler, warped_pos * 1.0).r;
        let detail_v = textureSample(perlin_worley_tex, noise_sampler, warped_pos * 3.0).r;

        // Density model: coverage threshold + detail
        let base_density = max(worley_v - (1.0 - coverage), 0.0);
        let detail_density = detail_v * 0.4;

        // Height shaping: stronger clouds at mid-altitude
        let height_factor = 1.0 - abs(height_norm - 0.5) * 2.0;
        let density = (base_density + detail_density) * height_factor * weather_val * density_scale;

        if (density > 0.001) {
            // --- Self-shadowing: secondary march toward sun ---
            let shadow_dt = 20.0 / shadow_steps;
            var shadow_od: f32 = 0.0;
            for (var s: f32 = 0.0; s < shadow_steps; s += 1.0) {
                let sp = pos + sun_dir * ((s + 0.5) * shadow_dt);
                let s_noise = sp * 0.008 + wind * 0.01;
                let s_worley = textureSample(worley_tex, noise_sampler, s_noise).r;
                let s_detail = textureSample(perlin_worley_tex, noise_sampler, s_noise * 3.0).r;
                let s_base_d = max(s_worley - (1.0 - coverage), 0.0);
                let s_d = (s_base_d + s_detail * 0.4) * weather_val * density_scale;
                shadow_od += s_d * shadow_dt;
            }
            let sun_trans = beer_transmittance(shadow_od * 0.3);

            // --- Extinction ---
            let absorption = density * 0.15;
            transmittance *= exp(-absorption * dt);

            // --- Phase function (double-lobe HG) ---
            let cos_theta = dot(ray_dir, sun_dir);
            let phase_forward = henyey_greenstein(cos_theta, g_forward);
            let phase_back = henyey_greenstein(cos_theta, -g_back);
            let phase = phase_forward * 0.7 + phase_back * 0.3;

            light_energy += density * dt * sun_trans * phase * 3.0 * transmittance;
        }
    }

    let alpha = 1.0 - transmittance;

    // Cloud color: blend between low-altitude warm and high-altitude cool
    let height_norm = (0.5 * (min_y + max_y) - min_y) / (max_y - min_y); // mid-slab
    let cloud_color = mix(clouds.cloud_color_low.rgb, clouds.cloud_color_high.rgb, height_norm);

    return vec4<f32>(light_energy * cloud_color, alpha);
}
```

- [ ] **Step 2: Build and test**

```bash
cargo test --workspace --lib
cargo build --release
```

Expected: all tests pass, release builds clean (WGSL compiles)

- [ ] **Step 3: Commit**

```bash
git add crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs
git commit -m "feat(cloud): new WGSL shader with multi-noise, HG phase, self-shadowing"
```

---

### Task 6: Quality preset routing

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`

**Interfaces:**
- Consumes: `CloudConfig.quality`
- Produces: quality-dependent texture sizes + uniform params written in `apply_frame()`

- [ ] **Step 1: Add quality-to-params mapping**

In `mod.rs`, add a helper method:

```rust
impl VolumetricCloudPass {
    fn quality_params(quality: &CloudQuality) -> glam::Vec4 {
        match quality {
            CloudQuality::Low => glam::Vec4::new(32.0, 4.0, 0.85, 0.3),
            CloudQuality::Medium => glam::Vec4::new(64.0, 6.0, 0.85, 0.3),
            CloudQuality::High => glam::Vec4::new(128.0, 8.0, 0.85, 0.3),
        }
    }
}
```

- [ ] **Step 2: Write quality_params into uniform in apply_frame()**

In `apply_frame()`, after `self.time += frame.delta_time * cfg.wind_speed;`:

```rust
let quality = cfg.quality;
let quality_params = Self::quality_params(&quality);
```

Add `quality_params` to `CloudUniform { ... quality_params, ... }`.

- [ ] **Step 3: Add test for quality routing**

In the test module:

```rust
#[test]
fn quality_preset_maps_to_step_counts() {
    let low = VolumetricCloudPass::quality_params(&CloudQuality::Low);
    let med = VolumetricCloudPass::quality_params(&CloudQuality::Medium);
    let high = VolumetricCloudPass::quality_params(&CloudQuality::High);
    assert!(low.x < med.x);
    assert!(med.x < high.x);
}

#[test]
fn cloud_pass_writes_quality_to_uniform() {
    let (device, queue) = headless_device_queue();
    let mut pass = VolumetricCloudPass::new(&device, &queue);
    let mut world = World::new();
    world.spawn((CloudComponent {
        config: CloudConfig { quality: CloudQuality::High, ..CloudConfig::default() },
    },));
    let optional = extract_optional_pass_data(&world);
    // … construct RenderFrame, call apply_frame …
    // verify uniform buffer was written (can check indirectly via pass.has_clouds)
    assert!(pass.has_clouds);
}
```

- [ ] **Step 4: Full test run + commit**

```bash
cargo test --workspace --lib
git add crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs
git commit -m "feat(cloud): quality preset routing from CloudConfig to shader"
```

---

### Task 7: Integration — variable noise texture sizes per quality
**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs`

- [ ] **Step 1: Make noise generation quality-aware in new()**

In `VolumetricCloudPass::new()`, accept `quality: CloudQuality` as a parameter and use it to determine noise texture dimensions. Current `new(device, queue)` signature stays but adds quality param. For `init(ctx)`, default to Medium.

- [ ] **Step 2: Full test run + visual verify**

```bash
cargo test --workspace --lib
cargo build --release
```

- [ ] **Step 3: Update progress ledger + commit**

```bash
git add crates/aether-engine/src/renderer/passes/volumetric_cloud/
git commit -m "feat(cloud): variable noise texture sizes per quality preset"
```

---

## Test Summary

| Task | New Tests | Test Focus |
|------|-----------|------------|
| T1 | 3 | Worley size, range, determinism; Perlin-Worley size, range, determinism |
| T2 | 3 | Curl size, range, determinism |
| T3 | 2 | Weather map size, range; CloudConfig serialization |
| T4 | 1 | Multi-noise texture dimensions in pass |
| T5 | 0 | WGSL compilation verified via pipeline creation |
| T6 | 2 | Quality param mapping; uniform write |
| T7 | 0 | Integration / visual |
| **Total** | **11** | |
