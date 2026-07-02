# Volumetric Cloud Redesign — Full-Grade Upgrade

Date: 2026-06-29

## 1. Goal

Replace the current single-layer FBM value-noise cloud with an industry-standard
multi-octave volumetric cloud pipeline (Nubis / HZD 2015 style), with three
quality presets (Low / Medium / High) configurable at runtime.

## 2. Current State (baseline)

- One 64³ R8Unorm 3D FBM value-noise texture (4 octaves, lacunarity 2.0, gain 0.5)
- 32-step primary ray march through horizontal slab [min_y, max_y]
- Simple lighting: `dot(ray_dir, sun_dir) * 0.5 + 0.5`
- Fixed cloud color: `vec3(1.0, 0.98, 0.95)`
- No self-shadowing, no multi-layer noise, no weather map
- Density: `max(noise - (1-coverage), 0) * density_scale`

## 3. Target Architecture

### 3.1 Noise Textures (offline pre-computed, CPU generation at init)

All textures are R8Unorm (3D) or R8Unorm (2D), generated once at `VolumetricCloudPass::new()`.

| Texture | Size | Memory | Purpose |
|---------|------|--------|---------|
| Low-freq Worley 3D | 128³ | ~2 MB | Large cloud shapes / billow clusters |
| High-freq Perlin-Worley 3D | 128³ | ~2 MB | Fine fluffy detail erosion |
| Curl noise 3D | 16³ (Low) / 32³ (High) | ~4 KB / ~32 KB | Turbulence warp for wispy edges |
| Weather map 2D | 64² | ~4 KB | Coverage distribution mask |

No external files. All generated programmatically in `clouds/noise.rs`.

### 3.2 Ray Marching Model (fragment shader)

```
for each pixel:
  project ray through cloud slab [bottom_altitude, top_altitude]
  clamp t_exit by geometry depth
  march N steps (32/64/128)
  at each step:
    sample_base = remap(Worley, PerlinWorley, weather, height) → density
    curl_offset = sample(curl_noise, pos * 0.02 + wind) → warp
    density = base density + detail density
    // Self-shadowing: secondary march toward sun (4/6/8 steps)
    sun_transmittance = beer(shadow_march())
    // Phase + extinction
    extinction = density * absorption_coeff
    transmittance *= exp(-extinction * dt)
    light = sun_color * sun_transmittance * phase(ray_dir, sun_dir, g_forward, g_back)
    energy += transmittance * density * dt * light
```

### 3.3 Lighting Model

- **Beer's Law**: transmittance = exp(-absorption × optical_depth)
- **Henyey-Greenstein double-lobe phase function**:
  ```
  phase = blend * HG(cos_θ, g_forward) + (1-blend) * HG(cos_θ, -g_back)
  ```
  g_forward ≈ 0.85 (silver lining), g_back ≈ 0.3 (soft back-scatter)
- **Ambient**: sample top/bottom hemisphere to approximate multi-scatter ambient
- **Cloud color**: lerp between warm-white (low altitude) and cool-blue (high altitude)
  based on height gradient + optional `atmosphere_ambient` uniform from AtmospherePass
- **Powder effect** (optional Medium+): `1 - exp(-density * 2)` to darken cloud edges

### 3.4 Composite Integration

Current: Cloud writes to `CloudColor` (Rgba16Float), composited later.

**No change needed.** The output format, resource tag, and composite position remain
identical. Only the shader and noise inputs change.

## 4. CloudQuality Enum

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CloudQuality {
    Low,
    Medium,
    High,
}
```

Serialized in RON as `"Low"`, `"Medium"`, or `"High"` (serde string variant names).

## 5. Quality Presets

| Parameter | Low | Medium (default) | High |
|-----------|-----|-------------------|------|
| Primary steps | 32 | 64 | 128 |
| Shadow steps | 4 | 6 | 8 |
| Worley size | 64³ | 128³ | 128³ |
| Perlin-Worley size | 64³ | 128³ | 128³ |
| Curl noise | 16³ | 16³ | 32³ |
| Weather map | 32² | 64² | 64² |
| Powder effect | Deferred | Deferred | Deferred |
| Ambient approx | Deferred | Deferred | Deferred |

**Powder effect**: `density = 1.0 - exp(-density * 2.0)` applied after all noise
sampling but before extinction. Creates dark edges on cloud silhouettes.

**Ambient approx**: sample 6 cone directions (top hemisphere + sun-aligned)
with single noise lookup each, average for a low-frequency ambient term
replacing the hard-coded 0.5 dark-side floor.

Preset is stored in `CloudConfig` as `quality: CloudQuality` enum, serialized in
scene RON files. Launcher UI can override at runtime.

## 6. File Changes

### New files
- `crates/aether-engine/src/renderer/clouds/worley.rs` — cellular/Worley noise
- `crates/aether-engine/src/renderer/clouds/perlin_worley.rs` — Perlin × Worley blend
- `crates/aether-engine/src/renderer/clouds/curl.rs` — curl noise from Perlin gradient

### Modified files
| File | Changes |
|------|---------|
| `clouds/noise.rs` | Refactor: expose `generate_*` functions; keep FBM for weather map |
| `clouds/mod.rs` | Export new modules |
| `passes/volumetric_cloud/types.rs` | Add `CloudQuality` enum, extend `CloudConfig` |
| `passes/volumetric_cloud/pipeline.rs` | New WGSL shader, multi-texture bind group, phase lighting |
| `passes/volumetric_cloud/mod.rs` | Wire quality preset, generate multi-noise textures |
| `passes/volumetric_cloud/execute.rs` | Bind all noise textures |
| `scene/config/clouds.rs` | Add `quality` field |

## 7. Risks / Unknowns

- **128³ × 2 textures ≈ 4MB GPU memory** — acceptable for modern GPUs; Low preset drops to 64³
- **CPU noise generation time**: 128³ Worley is ~0.5s single-threaded. Use `rayon` or accept
  startup delay. For now, synchronous generation is acceptable.
- **Atmosphere coupling**: cloud color blending uses a `atmosphere_ambient` uniform field.
  If AtmospherePass is disabled, fall back to `vec3(0.3, 0.4, 0.6)` default sky color.

## 8. Acceptance Criteria

1. Visual: clouds show billowy multi-scale detail (not uniform blob), silver lining on sun-facing edges
2. Quality presets: Low/Medium/High toggle via config, visible step count difference
3. Tests: `generate_worley_noise` returns correct dimensions, all noise values in [0,1]
4. Existing tests pass; `CloudColor` output format and resource pipeline unchanged
5. Release build compiles without new warnings
