# Task 5 Report: New WGSL shader with HG phase + self-shadowing

**Status:** DONE

**Commit:** `6b428fb` — feat(cloud): new WGSL shader with multi-noise, HG phase, self-shadowing

## What changed

- Replaced the WGSL source in `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs` with the new multi-noise ray-marcher from the task brief.
- The new fragment shader:
  - Samples the weather map for coverage.
  - Uses Worley + Perlin-Worley for base/detail density.
  - Applies curl-noise position warping.
  - Marches a secondary ray toward the sun for self-shadowing.
  - Applies a double-lobe Henyey-Greenstein phase function for silver-lining.
  - Blends `cloud_color_low` / `cloud_color_high` based on slab midpoint.
- Kept the same vertex stage (`vs_main`) and fragment output signature (`@location(0) vec4<f32>`).
- Preserved the legacy `@group(1) @binding(1) noise_tex` declaration (unused in the new path).

## Verification

```bash
cargo test --workspace --lib
```

Result: **199 passed; 0 failed; 0 ignored**.

```bash
cargo build --release
```

Result: **Finished `release` profile successfully** (only pre-existing deprecation warnings).

The shader compiles through wgpu because the existing `volumetric_cloud` tests exercise pass initialization, which creates the render pipeline and therefore validates the WGSL module.

## Important review findings (post-implementation fixes)

Two issues were identified in review and fixed in `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`.

### Fix 1: Explicit-LOD sampling in non-uniform shadow march

The self-shadowing secondary ray is nested inside `if (density > 0.001)`, which is non-uniform control flow. WGSL requires that `textureSample` calls inside such regions use an explicit LOD. The two shadow samples were changed from `textureSample` to `textureSampleLevel(..., 0.0)`:

- `textureSample(worley_tex, noise_sampler, s_noise)` → `textureSampleLevel(worley_tex, noise_sampler, s_noise, 0.0)`
- `textureSample(perlin_worley_tex, noise_sampler, s_noise * 3.0)` → `textureSampleLevel(perlin_worley_tex, noise_sampler, s_noise * 3.0, 0.0)`

### Fix 2: Correct curl-noise remapping

The curl texture is uploaded as `Rg8Snorm`, so `textureSample` already returns signed values in `[-1, 1]`. The previous code remapped the sampled red/green channels with `* 2.0 - 1.0`, which double-remapped the data. It was simplified to use the sampled values directly:

```wgsl
let curl_sample = textureSample(curl_tex, noise_sampler, noise_pos * 0.02).rg;
let warp = vec3<f32>(curl_sample.r, 0.0, curl_sample.g);
```

### Re-verification

```bash
cargo test --workspace --lib
```

Result: **199 passed; 0 failed; 0 ignored**.

```bash
cargo build --release
```

Result: **Finished `release` profile successfully** (only pre-existing deprecation warnings).

### Files touched

- `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
- `.superpowers/sdd/task-5-report.md` (this report)

### Commit

`79cc0e1` — fix(cloud): explicit LOD in shadow march + correct curl-noise remapping

- The brief listed `pipeline.rs` as the file to modify, but the actual shader source lives in `shader.rs` (`pub(crate) const SHADER`), which `pipeline.rs` imports. I edited `shader.rs`, consistent with the task context provided by the parent agent.
- `CloudUniform` layout matches `types.rs` from Task 4; no Rust-side changes were needed.
- Bind group declarations align with the layout created in `pipeline.rs`:
  - Group 1 binding 2 is declared as `depth_sampler` in WGSL and remains unused; the pipeline layout still exposes a filtering sampler at that binding.
  - Group 2 binding 4 sampler is renamed `noise_sampler` in WGSL; the Rust-side resource bound there is `multi_noise_sampler`, which is fine because only the binding index matters.
### Fix 3: Slab entry/exit distances for upward and downward rays

The original slab intersection computed the lower and upper ray distances as fixed `t_min` and `t_max` without accounting for the sign of `ray_dir.y`:

```wgsl
let t_min = (min_y - clouds.camera_pos.y) / ray_dir.y;
let t_max = (max_y - clouds.camera_pos.y) / ray_dir.y;
```

When `ray_dir.y` is negative, the division flips the ordering, so the computed `t_min` can be larger than `t_max`. This caused downward-looking rays (e.g., looking toward the ground) to miss the cloud slab or to compute a negative/empty interval.

The logic at `shader.rs:63-73` was replaced with the signed-correct version that sorts the two distances after division:

```wgsl
let t1 = (min_y - clouds.camera_pos.y) / ray_dir.y;
let t2 = (max_y - clouds.camera_pos.y) / ray_dir.y;
let t_min = min(t1, t2);
let t_max = max(t1, t2);
if (t_max < 0.0) {
    return vec4<f32>(0.0);
}

var t_enter = max(t_min, 0.0);
var t_exit = t_max;
if (t_enter > t_exit) {
    return vec4<f32>(0.0);
}
```

This now correctly handles both upward (`ray_dir.y > 0`) and downward (`ray_dir.y < 0`) rays by always entering at the nearer signed distance and exiting at the farther one, then clamping the entry to the camera plane (`max(t_min, 0.0)`).

### Re-verification

```bash
cargo test --workspace --lib
```

Result: **199 passed; 0 failed; 0 ignored**.

```bash
cargo build --release
```

Result: **Finished `release` profile successfully** (only pre-existing deprecation warnings).

### Files touched

- `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs`
- `.superpowers/sdd/task-5-report.md` (this report)

