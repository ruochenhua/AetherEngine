# Cloud Shell Tangent Clipping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the horizontal volumetric-cloud cutoff when the camera is just above the cloud bottom altitude.

**Architecture:** Return two ordered ray-distance intervals from the WGSL shell-intersection function. Clip both intervals by geometry depth, then raymarch their union with one stable sample sequence so the image stays continuous through the inner-sphere tangent.

**Tech Stack:** Rust, WGSL, wgpu, naga shader validation, release launcher screenshot capture.

## Global Constraints

- Keep the change confined to the volumetric-cloud shader.
- Preserve the existing cloud configuration and render-pass resource interface.
- Register no new shader; the existing source remains in `SHADER_MANIFEST`.
- Run the Aether Engine cloud verification checklist before completion.

---

### Task 1: Establish the visual regression signal

**Files:**
- Create temporarily: `.codex-cloud-repro-50_8.ron`
- Create temporarily: `.codex-cloud-before.png`

**Interfaces:**
- Consumes: launcher CLI `--scene`, `--screenshot`, `--exit-after-frames`
- Produces: deterministic screenshot showing the tangent cutoff

- [ ] **Step 1: Create a frozen scene**

Use `scenes/13_clouds.ron` parameters with camera position `(0.0, 50.8, 0.0)`,
wind speed `0.0`, and no terrain so only the cloud/atmosphere boundary is
measured.

- [ ] **Step 2: Capture the failing image**

Run:

```powershell
target\release\aether-launcher.exe --scene .codex-cloud-repro-50_8.ron --screenshot E:\Projects\AetherEngine\.codex-cloud-before.png --exit-after-frames 120 --no-gui-overlay
```

Expected: launcher exits successfully and the screenshot contains the hard
horizontal cutoff just below the center of the image.

### Task 2: Represent both cloud-shell intervals

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs:315`

**Interfaces:**
- Consumes: ray origin/direction, inner and outer cloud radii
- Produces: `CloudIntervals { near: vec2<f32>, far: vec2<f32> }`

- [ ] **Step 1: Replace the single-interval intersection**

Add a WGSL `CloudIntervals` struct. Intersect the outer sphere, subtract the
positive inner-sphere interval, and return zero-length intervals for absent
segments.

- [ ] **Step 2: Clip both intervals**

In `fs_main`, clamp both interval ends by reconstructed geometry distance and
discard only when both intervals are empty.

- [ ] **Step 3: March the interval union**

Change `front_to_back_raymarch` to consume the ray and two distance intervals.
Use one ordered sample sequence spanning the valid intervals and skip samples
that lie in the clear inner-sphere gap.

- [ ] **Step 4: Validate WGSL**

Run:

```powershell
cargo test -p aether-engine shader_validation --lib
```

Expected: all shader validation tests pass.

### Task 3: Prove the visual fix

**Files:**
- Create temporarily: `.codex-cloud-after.png`

**Interfaces:**
- Consumes: the frozen scene from Task 1
- Produces: post-fix screenshot for direct comparison

- [ ] **Step 1: Build the release launcher**

Run:

```powershell
cargo build -p aether-launcher --release
```

Expected: exit code 0.

- [ ] **Step 2: Capture the repaired image**

Repeat the Task 1 launcher command with output
`E:\Projects\AetherEngine\.codex-cloud-after.png`.

Expected: the cloud silhouettes cross the former tangent line without a
screen-wide cutoff.

- [ ] **Step 3: Inspect before/after side by side**

Confirm that the change removes the horizontal discontinuity without moving the
configured cloud bottom or introducing geometry bleed-through.

### Task 4: Run the project verification checklist

**Files:**
- Remove temporary repro scene and screenshots after inspection.

**Interfaces:**
- Consumes: completed shader change
- Produces: unit-test, build, runtime, and visual evidence

- [ ] **Step 1: Run engine tests**

```powershell
cargo test -p aether-engine --lib
```

Expected: exit code 0 with no failing tests.

- [ ] **Step 2: Run the affected scene**

```powershell
target\release\aether-launcher.exe --scene scenes\13_clouds.ron --screenshot E:\Projects\AetherEngine\.codex-cloud-final.png --exit-after-frames 120 --no-gui-overlay
```

Expected: the launcher reaches the capture frame without panic and clouds/sky
render correctly.

- [ ] **Step 3: Clean up and review**

Delete only `.codex-cloud-*` temporary artifacts, inspect `git diff`, and verify
that no debug instrumentation or unrelated change remains.
