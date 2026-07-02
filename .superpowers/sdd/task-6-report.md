# Task 6 Report: CloudQuality Preset Routing

## Summary

Wired the `CloudQuality` enum into the volumetric cloud pass so that `apply_frame()` writes quality-dependent step counts into the `CloudUniform.quality_params` shader uniform. Updated `scenes/13_clouds.ron` to request `quality: High`.

## Changes

### `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`

- Imported `CloudConfig` and `CloudQuality` from `crate::scene::config`.
- Added `VolumetricCloudPass::quality_params(quality: &CloudQuality) -> glam::Vec4` helper mapping:
  - `Low`    → `(32.0, 4.0, 0.85, 0.3)`
  - `Medium` → `(64.0, 6.0, 0.85, 0.3)`
  - `High`   → `(128.0, 8.0, 0.85, 0.3)`
- In `apply_frame()`, after advancing `self.time`, read `clouds.config.quality` and write the resulting `quality_params` into the `CloudUniform` instead of the previous default value.
- Kept `cloud_color_low` and `cloud_color_high` at their existing defaults.

### Tests added

- `quality_preset_maps_to_step_counts`: asserts the primary step count (`.x`) increases monotonically across Low/Medium/High.
- `cloud_pass_writes_quality_to_uniform`: constructs a `RenderFrame` with a `Clouds` component whose `quality` is `High`, calls `apply_frame`, and verifies `pass.has_clouds` is set (indirect confirmation the uniform buffer write path was reached).

### `scenes/13_clouds.ron`

- Added `quality: High` to the `clouds` block. Verified the file still parses and deserializes to `CloudQuality::High`.

## Verification

- Focused tests:
  - `cargo test -p aether-engine volumetric_cloud --lib`
  - Result: 6 passed, 0 failed
- Full suite:
  - `cargo test --workspace --lib`
  - Result: 201 passed, 0 failed
- Temporary parse check confirmed `scenes/13_clouds.ron` loads with `CloudQuality::High`.

## Notes / Concerns

- Texture sizes were **not** varied by quality. The brief’s explicit steps only covered uniform parameter routing, and the pipeline currently hard-codes noise texture dimensions. Making sizes quality-dependent would require constructor changes and larger data generation, which was outside the brief’s scope.
- The `quality_params` values match the task brief (`32/64/128` primary steps), not the alternate values shown in the higher-level context description (`48/64/96`).
