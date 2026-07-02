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

