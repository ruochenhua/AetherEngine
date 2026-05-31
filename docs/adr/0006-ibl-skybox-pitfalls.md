# ADR-0006: IBL + Skybox Implementation Pitfalls

## Status

Accepted

## Context

Phase 2 work: Image-Based Lighting (IBL) and skybox rendering. Follows the LearnOpenGL PBR/IBL
tutorial pipeline: equirectangular HDR → cubemap → irradiance convolution → prefiltered mip chain → BRDF LUT.

The implementation went through many iterations due to several subtle issues
spanning BRDF unification, projection matrix correction, coordinate system
mismatches, and image loading quirks. This ADR documents every pitfall so
future work (SSR, atmosphere, etc.) can build on solid ground.

---

## 1. BRDF Unification (Cook-Torrance)

### Problem

Direct lighting used **Blinn-Phong**, while IBL used **Cook-Torrance (GGX)** via split-sum
approximation. The specular lobe shapes from the two sources did not match —
rough metals in particular looked inconsistent.

### Fix

Unified all lighting to Cook-Torrance (GGX NDF + Smith G + Schlick Fresnel).
The lighting shader now uses the same BRDF functions as the IBL split-sum:

- `distribution_ggx(NdotH, roughness)` — Trowbridge-Reitz NDF
- `geometry_smith(NdotV, NdotL, roughness)` — Schlick-GGX with `k = (roughness+1)²/8`
- `fresnel_schlick(VdotH, F0)` — Schlick approximation

Direct diffuse now uses energy-conserving `kD = (1.0 - F) * (1.0 - metallic)` instead
of the previous hardcoded `(1.0 - metallic) * 0.96`. The same `F` and `kD` are shared
between direct lighting and IBL.

**Files:** `passes/lighting.rs` (WGSL shader), `docs/adr/0005-unified-cook-torrance-brdf.md`

---

## 2. Capture Projection Matrix (Critical Bug)

### Problem

The original `capture_projection()` in `ibl.rs` was broken in TWO ways:

**a) Wrong matrix multiplication order:**
```rust
// WRONG:
(p_gl * correction).to_cols_array()
```
This applied the z-correction BEFORE the perspective projection (to view-space
coordinates instead of clip-space), which is semantically wrong and produced
incorrect w values.

**b) Wrong correction formula:**
The correction matrix mapped z incorrectly, making `w_clip` **half** of the
correct value. This caused `x/w` and `y/w` (NDC) to **double** — each cubemap
face captured only ~45° instead of the required 90°, showing half the sky.

### Fix

```rust
// CORRECT:
// z_wgpu_ndc = (z_gl_ndc + 1) / 2 = (z_gl_clip + w_gl_clip) / (2 * w_gl_clip)
// So: z' = z + w,  w' = 2w
// Also compensate x,y: x' = 2x, y' = 2y (so x/w stays unchanged)
let correction = Mat4::from_cols_array(&[
    2.0, 0.0, 0.0, 0.0,
    0.0, 2.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 1.0, 2.0,
]);
(correction * p_gl).to_cols_array()
```

### Why it matters

Every subsequent cubemap (irradiance, prefiltered) is derived from the
environment cubemap. If the equirect→cubemap conversion is wrong, EVERYTHING
is wrong. This was the root cause of most visual issues during IBL development.

**Lesson:** When the correction is `z' = z + w, w' = 2w`, x and y MUST also
scale by 2 to keep `x/w` unchanged. The correction affects ALL components
through the homogeneous divide, not just z.

---

## 3. Image Loading Vertical Flip

### Problem

The `image` crate loads images with origin at **top-left** (standard for
most image formats). However, HDR equirectangular maps use a convention where
the origin is at **bottom-left** (like OpenGL textures). The loaded image is
therefore flipped vertically.

In the equirectangular sampling shader:
```wgsl
v_uv = asin(v.y) / PI + 0.5  // v.y = 1 → v_uv = 1.0 (top of loaded image)
```
Since the loaded image is flipped, `v_uv = 1.0` maps to the **bottom** of the
original HDR (ground instead of sky). This causes the +Y cubemap face to show
ground at the zenith.

### Fix

Swap the +Y and -Y cubemap faces by rendering with swapped view matrices in
`capture_views()`:

```rust
// +Y cubemap layer ← rendered with -Y view (looking down)
look_at([0,0,0], [0,-1,0], [0,0,-1])
// -Y cubemap layer ← rendered with +Y view (looking up)
look_at([0,0,0], [0, 1,0], [0,0, 1])
```

This makes the skybox show the correct content. However, since all cubemaps
(env, irradiance, prefiltered) share this swap, the **specular IBL reflection**
on metallic objects also gets vertically flipped.

### IBL Reflection Compensation

The prefiltered cubemap (used for specular IBL) inherits the Y-face swap.
To compensate, negate the Y component when sampling the prefiltered map:

```wgsl
// In lighting shader, specular IBL section:
let prefiltered_color = textureSampleLevel(
    prefiltered_map, ibl_sampler,
    vec3<f32>(R.x, -R.y, R.z),  // ← negate Y
    mip_level
).rgb;
```

This only affects the specular reflection direction, not the cubemap content.
The skybox samples the env cubemap directly (without Y flip) because the
content swap already corrects for the skybox view.

### Why not fix at the source?

We could fix the image flip in `sample_spherical_map` by using `asin(-v.y)`.
This was tested but caused other faces (especially +X, -X, +Z, -Z) to appear
horizontally mirrored due to the Vulkan cubemap face convention interaction.
The face-content-swap approach (capture_views) + per-consumer compensation
(skybox: no flip needed; IBL specular: flip Y) proved more reliable.

---

## 4. Skybox View Direction Reconstruction

### Problem

For sky pixels, the world-space view direction was computed as:
```wgsl
// WRONG:
let V_sky = normalize(world_ray.xyz / world_ray.w);
```
This gives the direction from the **world origin** to the reconstructed point,
not from the **camera**. With the camera at `(3, 3, 3)`, the resulting
direction was dominated by the camera's position, making it nearly constant —
the sky appeared as a single color that didn't change with camera rotation.

### Fix

```wgsl
// CORRECT:
let world_pos = world_ray.xyz / world_ray.w;
let V_sky = normalize(world_pos - uniforms.camera_pos);
```

Subtract `camera_pos` to get the direction from the camera to the world point.
Also added `inv_view_proj` to the `LightingUniforms` struct (computed each
frame from `(proj * view).inverse()`).

---

## 5. Skybox Tone Mapping Flicker

### Problem

The sky pixel path had its own tone mapping AND an early return:
```wgsl
if (is_sky) {
    let sky_color = sample_env_map(...);
    let mapped = sky_color / (sky_color + 1.0);  // separate tone map
    return vec4(mapped, 1.0);                     // early return
}
// ... geometry lighting with ITS OWN tone map at the end
```

At geometry-sky boundaries, the two tone-mapping instances produced slightly
different results because the HDR values going into tone mapping differed.
Pixels at the boundary flickered as the camera moved by sub-pixel amounts.

### Fix

Unified to a single tone mapping at the end of the shader:
```wgsl
var output_color: vec3<f32>;
if (is_sky) {
    output_color = sample_env_map(...);  // no tone map, no early return
} else {
    // ... geometry lighting
    output_color = final_color;
}
// Shared tone mapping
let mapped = output_color / (output_color + 1.0);
return vec4(mapped, 1.0);
```

---

## 6. Cubemap Face Orientation (Lessons Learned)

### What we tried (and failed)

Attempted to analytically determine per-face flips by comparing the Vulkan
cubemap sampling convention against the rendering's framebuffer orientation.
Spent many iterations adding `flip_x`/`flip_y`/`flip_z` uniforms based on
mathematical analysis, but consistently got faces wrong because:

1. **No visual feedback loop**: The agent cannot see the rendered output,
   so all corrections were based on theory without verification.
2. **The Vulkan cubemap convention** defines specific (s,t) ↔ (x,y,z) mappings
   for each face. Our rendering with `look_at_rh` + `perspective_rh` produces
   a framebuffer that must match this convention. The analytical approach works
   but requires precise attention to the view matrix's row composition.
3. **The `image` crate flip** complicates things: the equirect content is
   vertically flipped, so the "correct" rendering looks wrong.

### What actually worked

Minimal changes based on visual inspection (the user guided each fix):

1. Fixed `capture_projection()` — this was the root cause
2. Swapped Y faces in `capture_views()` — compensates for image crate flip
3. Negated `R.y` in specular IBL sampling — compensates for the Y swap in prefiltered cubemap
4. **No `flip_x`/`flip_y`/`flip_z` per-face corrections needed** — the Vulkan convention
   matches the rendering output for all X and Z faces with the original
   LearnOpenGL `capture_views()` up vectors

---

## 7. Input Manager Cursor Delta Bug

### Problem

When adding left-click-drag camera control, the camera rotation was jittery.
The mouse delta in `InputManager` was **overwritten** by each `CursorMoved`
event instead of **accumulated**:

```rust
// WRONG:
self.mouse_delta = (new_pos.0 - self.mouse_position.0, ...); // overwrites
```

Since winit sends multiple `CursorMoved` events per frame (one per display
refresh on some platforms), only the last event's tiny delta was preserved.

### Fix

```rust
// CORRECT:
self.mouse_delta.0 += new_pos.0 - self.mouse_position.0; // accumulates
self.mouse_delta.1 += new_pos.1 - self.mouse_position.1;
```

---

## Summary: Key Files Changed

| File | Change |
|------|--------|
| `passes/lighting.rs` | Cook-Torrance BRDF, unified kD, skybox sampling, shared tone mapping, specular IBL Y-flip |
| `crates/aether-engine/src/renderer/ibl.rs` | Fixed `capture_projection()`, Y-face swap in `capture_views()`, added `env_view` to `IblResources` |
| `renderer/light.rs` | Added `inv_view_proj` to `LightingUniforms` |
| `renderer/camera.rs` | Left-click-drag camera control, removed right-click toggle |
| `input.rs` | Fixed mouse delta accumulation |
| `scene/loader.rs` | Added `inv_view_proj` field initialization |
| `launcher/main.rs` | Updated debug modes (F1-F4 for IBL/skybox diagnostics), UI labels |
| `docs/adr/0005-unified-cook-torrance-brdf.md` | BRDF unification ADR |
| `docs/adr/0006-ibl-skybox-pitfalls.md` | This document |

---

## Golden Rules for Future Cubemap Work

1. **Fix the projection first.** If `capture_projection()` is wrong, nothing else matters.
2. **Don't fight the Vulkan convention.** The standard `capture_views()` up vectors
   from LearnOpenGL are correct for wgpu. Don't change them.
3. **Image-loading quirks are real.** Always verify whether the loaded image has
   the expected origin (top-left vs bottom-left).
4. **Per-consumer compensation is safer than modifying the cubemap.** When the
   cubemap has a systematic error (like the Y swap), fix it at the sampling site
   (skybox, IBL, etc.) rather than trying to "correct" the cubemap.
5. **Visual feedback beats analysis.** When working on visual features, rely on
   the user's eyes. Add debug modes (like F1-F4) to help isolate issues.
