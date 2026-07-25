# Cloud Shell Tangent Clipping Fix

## Problem

When the camera rises just above the configured cloud bottom altitude, the
volumetric-cloud image develops a horizontal cut at the tangent to the inner
cloud sphere. The current shader represents the shell intersection as one
`[start, end]` interval even though a ray can cross two cloud-bearing intervals
separated by the clear inner sphere.

## Design

Represent the shell intersection as two ordered ray-distance intervals. Build
them by intersecting the ray with the outer sphere and subtracting the inner
sphere interval. Clip both intervals against `max_render_dist` and opaque-scene
depth before marching.

March across the ordered interval span with one stable sample budget and sample
density only while the current ray distance belongs to either cloud interval.
This keeps the sample distribution continuous as the inner-sphere discriminant
passes through zero, while preserving front-to-back compositing and geometry
occlusion.

## Scope

- Modify only the volumetric-cloud raymarch shader.
- Do not change cloud appearance parameters, noise generation, or public scene
  configuration.
- Preserve shader-manifest validation.

## Verification

- Reproduce the old failure with a fixed camera at altitude `50.8` and cloud
  bottom altitude `50.0`.
- Confirm the repaired image has no horizontal cloud cutoff at the inner-sphere
  tangent.
- Compare fixed screenshots below and just above the cloud base.
- Run `cargo test -p aether-engine --lib`.
- Run `cargo build -p aether-launcher --release`.
- Launch and capture `scenes/13_clouds.ron` in release mode.

## Verification Results

- Fixed-height repro at camera altitude `50.8`: the screen-wide tangent cutoff
  is absent after the change.
- Maximum adjacent-row colour discontinuity in the central viewport fell from
  `7.93` to `3.73` (about 53%).
- Naga shader validation: 2 passed, 0 failed.
- `cargo test -p aether-engine --lib`: 274 passed, 0 failed.
- `cargo build -p aether-launcher --release`: succeeded.
- Release launcher rendered `scenes/13_clouds.ron` through frame 120 and saved
  the verification screenshot without panic.
