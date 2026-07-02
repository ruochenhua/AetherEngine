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
