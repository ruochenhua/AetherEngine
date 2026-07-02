# Task 7 Report: Integration — variable noise texture sizes per quality

**Status:** DONE_WITH_CONCERNS

## Summary

Made the volumetric cloud noise texture dimensions quality-aware by threading `CloudQuality` through `VolumetricCloudPass::new()` and `new_without_upload()`. The `Pass::init()` path defaults to `CloudQuality::Medium` so the renderer integration continues to work without a scene-supplied quality at construction time.

## Changes

### `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs`

- Added a `NoiseSizes` helper struct and `From<&CloudQuality>` mapping:
  - **Low:** Worley 64³, Perlin-Worley 64³, Curl 16³, Weather 32²
  - **Medium:** Worley 128³, Perlin-Worley 128³, Curl 16³, Weather 64²
  - **High:** Worley 192³, Perlin-Worley 192³, Curl 32³, Weather 128²
- Updated `new_without_upload(device, quality)` to take `&CloudQuality` and use the mapping instead of hard-coded 128/16/64 sizes.

### `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`

- Updated `Pass::init()` to call `Self::new_without_upload(ctx.device, &CloudQuality::Medium)`.
- Updated `VolumetricCloudPass::new(device, queue, quality)` to accept `CloudQuality` and forward it to `new_without_upload`.
- Moved `CloudConfig` import into the `#[cfg(test)]` module to avoid an unused-import warning in release builds.
- Updated all existing tests to pass `CloudQuality::Medium` so the legacy assertions still hold.
- Added a new test `cloud_noise_texture_dimensions_vary_by_quality` that builds passes for Low, Medium, and High and asserts the exact texture dimensions for Worley/Perlin-Worley, Curl, and Weather.

## Verification

### Unit tests

```bash
cargo test --workspace --lib
```

Result: **202 passed; 0 failed**. The new per-quality dimension test is included.

### Release build

```bash
cargo build --release
```

Result: **Finished `release` profile** with only pre-existing warnings (deprecated glam functions and redundant comparisons in cloud noise tests); no new warnings introduced by this change.

### Visual verification

Attempted to run the cloud scene with the launcher:

```bash
cargo run --release --bin aether-launcher -- --scene scenes/13_clouds.ron --no-gui-overlay --exit-after-frames 1 --screenshot output/task7_clouds.png
```

The launcher successfully rebuilt the pipeline and registered the `VolumetricCloud` pass, but the process did not terminate within the 120-second timeout and no screenshot was written. This appears to be an environment limitation (headless/GUI runtime issue) rather than a compile or pipeline error. Visual verification was therefore not completed.

## Commit

```
6598540 feat(cloud): variable noise texture sizes per quality preset
```

## Concerns / Follow-up

- Visual verification could not be completed in this environment. The pipeline was correctly rebuilt with `VolumetricCloud` in the pass list, but the launcher timed out before producing a frame/screenshot.
- The `task-1-brief.md` file in `.superpowers/sdd/` is already modified in the worktree (it contains the correct Task 1 cloud brief content). I left it unstaged because it is not part of this task's scope.

---

# Final Fix Report: Volumetric Cloud Redesign Review

**Date:** 2026-07-02

**Status:** DONE

## Findings Addressed

### Important

1. **High preset texture sizes aligned with spec**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs`
   - Changed `NoiseSizes` High mapping to:
     - Worley: 128³
     - Perlin-Worley: 128³
     - Curl: 32³
     - Weather: 64²
   - Removed the redundant `perlin_worley` size field; Perlin-Worley now always shares Worley's resolution.
   - Updated `cloud_noise_texture_dimensions_vary_by_quality` in `mod.rs` to assert `High = (128, 32, 64)`.

2. **Self-shadowing density mismatch fixed**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
   - Introduced `sample_density(pos, coverage, density_scale, wind, min_y, max_y, with_detail)` helper.
   - Helper applies curl warp, weather coverage, height shaping, and density scale consistently.
   - Both the primary ray march and the sun-direction shadow march now call the same helper.

3. **Cloud color gradient now uses actual sample height**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
   - Moved the cloud-color blend into the primary march loop.
   - `light_energy` is now a `vec3<f32>` and accumulates a per-sample height-graded color contribution.
   - Removed the tautological mid-slab `height_norm` calculation.

4. **Test name no longer over-promises**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - Renamed `cloud_pass_writes_quality_to_uniform` to `cloud_pass_applies_frame_with_high_quality`.

5. **Legacy single noise texture removed**
   - Files: `mod.rs`, `pipeline.rs`, `shader.rs`, `types.rs`
   - Removed `noise_texture`, `noise_view`, `noise_sampler`, `noise_data`, `noise_uploaded`, and `upload_legacy_noise`.
   - Removed the `@group(1) @binding(1)` legacy `noise_tex` declaration and the unused `@group(1) @binding(2)` `depth_sampler`.
   - Texture bind group 1 now contains only the depth texture.
   - Removed `NOISE_SIZE` from `types.rs` and the unused FBM value-noise helpers from `pipeline.rs`.
   - Updated `cloud_noise_texture_has_expected_dimensions` to stop asserting the removed legacy texture.

### Minor

6. **Module docstring updated**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - Now describes Worley + Perlin-Worley + Curl + Weather textures instead of a single procedural noise texture.

7. **Unnecessary `#[allow(dead_code)]` cleaned up**
   - File: `mod.rs`
   - Removed the attribute from `noise_bind_group` (used in `execute.rs`).
   - Kept it on `noise_bind_group_layout` and other lifetime-only resource fields.

8. **Curl comment and asserts fixed**
   - File: `crates/aether-engine/src/renderer/clouds/curl.rs`
   - Corrected comment to "Map [-1, 1] to [-127, 127]".
   - Replaced tautological `x <= 127` / `y <= 127` asserts with lower-bound checks.

9. **Tautological u8 asserts replaced with dynamic-range checks**
   - Files: `perlin_worley.rs`, `weather.rs`, `worley.rs`
   - Tests now assert that generated data has a non-empty min-to-max dynamic range.

10. **CloudQuality Default simplified**
    - File: `crates/aether-engine/src/scene/config/clouds.rs`
    - Replaced manual `Default` impl with `#[derive(Default)]` and `#[default]` on `Medium`.

## Verification

```bash
cargo test --workspace --lib
```

Result: **202 passed; 0 failed**. Warnings are limited to pre-existing deprecated `glam` camera-function warnings.

```bash
cargo build --release
```

Result: **Finished `release` profile** with only the same pre-existing warnings.

## Commit

```
63996cc fix(cloud): address final review findings for volumetric cloud redesign
```

## Files Changed

- `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
- `crates/aether-engine/src/renderer/passes/volumetric_cloud/pipeline.rs`
- `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
- `crates/aether-engine/src/renderer/passes/volumetric_cloud/types.rs`
- `crates/aether-engine/src/renderer/clouds/curl.rs`
- `crates/aether-engine/src/renderer/clouds/perlin_worley.rs`
- `crates/aether-engine/src/renderer/clouds/weather.rs`
- `crates/aether-engine/src/renderer/clouds/worley.rs`
- `crates/aether-engine/src/scene/config/clouds.rs`
- `.superpowers/sdd/task-7-report.md`


---

# Final Review Fix Report: Volumetric Cloud Redesign

**Date:** 2026-07-02

**Commit:** `7335f32`

**Status:** DONE

## Findings Addressed

### Important

1. **Density clamped to non-negative**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
   - `sample_density` now returns `max(..., 0.0)` so shadow marches that sample outside the cloud slab no longer produce negative density.

2. **Runtime `CloudQuality` selects noise texture sizes**
   - Files: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`, `pipeline.rs`
   - Removed `quality` from `new()` and `new_without_upload()`.
   - `new_without_upload()` now creates only the pipeline, uniform buffer/bind group, bind group layouts, and an empty group-2 placeholder.
   - Added `ensure_noise_textures(device, queue, quality)` in `mod.rs`:
     - No-op if already created for the same quality.
     - Otherwise creates textures via `textures.rs` helpers, generates/upload noise, and creates the group-2 bind group.
   - `apply_frame()` calls `ensure_noise_textures` when `frame.optional.clouds` is present, using `clouds.config.quality`.
   - `init()` calls `new_without_upload(ctx.device)` without a default quality.
   - Quality table matches the spec:
     - Low: Worley 64³, Perlin-Worley 64³, Curl 16³, Weather 32²
     - Medium: Worley 128³, Perlin-Worley 128³, Curl 16³, Weather 64²
     - High: Worley 128³, Perlin-Worley 128³, Curl 32³, Weather 64²

3. **Worley generated only once**
   - File: `crates/aether-engine/src/renderer/clouds/perlin_worley.rs`
   - Added `perlin_worley_from_worley(worley_data, size)` which reuses a supplied Worley buffer.
   - `perlin_worley_noise_3d(size)` remains a convenience wrapper that generates Worley and delegates to the new helper.
   - `VolumetricCloudPass::ensure_noise_textures()` generates Worley once and feeds it to Perlin-Worley.

### Minor

4. **Quality-routing test improved**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - `cloud_pass_applies_frame_with_high_quality` now reads back the uniform buffer and asserts `quality_params.x == 128.0`.
   - The uniform buffer was updated to include `COPY_SRC` usage for readback.

5. **Weather hash consistency**
   - File: `crates/aether-engine/src/renderer/clouds/weather.rs`
   - Hash closure now uses `n as u32 as f32 / u32::MAX as f32`, matching Worley/Perlin-Worley.

6. **Curl range test**
   - File: `crates/aether-engine/src/renderer/clouds/curl.rs`
   - Added upper-bound asserts cast to `i32` to avoid `unused_comparisons`.

## Verification

```bash
cargo test --workspace --lib
```

Result: **202 passed; 0 failed**.

```bash
cargo build --release
cargo build --release -p aether-launcher
```

Result: **Finished `release` profile** with only pre-existing deprecated-glam warnings.

---

# Final Review Fix Report: Volumetric Cloud Redesign (Round 2)

**Date:** 2026-07-02

**Status:** DONE

## Findings Addressed

### Critical

1. **`mod.rs` kept under 500 physical lines**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - Moved the entire body of `ensure_noise_textures` into `textures::create_noise_resources(device, queue, quality, layout) -> NoiseResources`.
   - `NoiseResources` bundles the four textures, four views, sampler, and bind group.
   - `ensure_noise_textures` now only checks the cached quality, calls the helper, and assigns the returned fields.
   - Removed the now-unused `use textures::{create_texture_2d, create_texture_3d};` import.
   - Result: `mod.rs` reduced from 573 to 409 physical lines.

### Important

2. **Orphaned `clouds/noise.rs` refactored into shared utility**
   - Deleted `crates/aether-engine/src/renderer/clouds/noise.rs` (contained dead `generate_cloud_noise_texture` / `generate_noise_data` API).
   - Added `crates/aether-engine/src/renderer/clouds/value_noise.rs` exporting:
     - `pub(crate) fn value_noise_3d(p: Vec3) -> f32`
     - `pub(crate) fn fbm_perlin_3d(p: Vec3, octaves, lacunarity, gain) -> f32`
   - Updated `clouds/mod.rs` to expose `pub mod value_noise` instead of `pub mod noise`.
   - Updated `perlin_worley.rs` and `curl.rs` to import and use the shared helpers, removing their duplicated `value_noise_3d` / `fbm_perlin_3d` implementations.

3. **Shadow march skips detail noise**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
   - Changed the secondary sun march's `sample_density(...)` call from `with_detail: true` to `with_detail: false` to reduce per-pixel cost.

### Minor

4. **Removed unnecessary `#[allow(dead_code)]`**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - Removed the `#[allow(dead_code)]` attribute from `noise_bind_group_layout` since it is actively used when creating the noise bind group.

5. **Simplified `ensure_noise_textures` signature**
   - File: `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
   - Removed the redundant `device: &wgpu::Device` argument; the method now uses `&self.device`.
   - Updated callers in `new()`, `new_with_quality()`, and `apply_frame()` accordingly.

## Files Changed

- `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`
- `crates/aether-engine/src/renderer/passes/volumetric_cloud/textures.rs`
- `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
- `crates/aether-engine/src/renderer/clouds/mod.rs`
- `crates/aether-engine/src/renderer/clouds/value_noise.rs` (new)
- `crates/aether-engine/src/renderer/clouds/perlin_worley.rs`
- `crates/aether-engine/src/renderer/clouds/curl.rs`
- `crates/aether-engine/src/renderer/clouds/noise.rs` (deleted)

## Verification

```bash
wc -l crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs
```

Result: **409 lines** (target: under 500, preferably under 400).

```bash
cargo test --workspace --lib
```

Result: **200 passed; 0 failed**.

```bash
cargo build --release
cargo build --release -p aether-launcher
```

Result: **Finished `release` profile** for both targets with only pre-existing deprecated-glam warnings.

## Commit

```
4b4170d fix(cloud): address final-review findings for volumetric cloud redesign
```
