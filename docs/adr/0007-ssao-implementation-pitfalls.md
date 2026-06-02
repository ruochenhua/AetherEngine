# ADR-0007: SSAO Implementation Pitfalls

## Status

Accepted

## Context

Phase 2 work: Screen-Space Ambient Occlusion (SSAO). Follows the LearnOpenGL SSAO
approach: hemisphere kernel in view space, random rotation per pixel, depth
comparison via GBuffer position reprojection.

A code review revealed several issues causing incorrect visual output.
This ADR documents the root causes and the fixes applied.

---

## 1. Reprojection UV Y-Flip (Critical)

### Problem

SSAO reprojects view-space sample offsets back to screen UVs to sample the
GBuffer position texture:

```wgsl
let sc = view_proj * vec4<f32>(sw, 1.0);
let suv = sc.xy / sc.w * 0.5 + 0.5;
```

In WebGPU:
- NDC `y = -1` is the **bottom** of the screen
- NDC `y = +1` is the **top** of the screen
- Texture UV `v = 0` is the **top** of the texture
- Texture UV `v = 1` is the **bottom** of the texture

The original code mapped NDC y directly to UV v, causing a **vertical flip**.
All 16 hemisphere samples read from the wrong vertical location, making the
occlusion calculation completely incorrect.

### Fix

Invert NDC y when computing UV v:

```wgsl
let suv = vec2<f32>(
    sc.x / sc.w * 0.5 + 0.5,
    -sc.y / sc.w * 0.5 + 0.5
);
```

This matches the inverse of the Lighting Pass UV→NDC transform:
`ndc.y = 1.0 - uv.y * 2.0`.

---

## 2. Inline Bilateral Blur Was Non-Functional

### Problem

The SSAO fragment shader attempted a 4-tap bilateral blur inline:

```wgsl
var blurred: f32 = occlusion;
var tw: f32 = 1.0;
// for each neighbor:
blurred += occlusion * w; tw += w;
occlusion = blurred / max(tw, 1.0);
```

A fragment shader **cannot sample its own color attachment** as an input texture.
The code used `occlusion` (the center pixel's value) for every tap, so:

```
blurred = occlusion * (1 + w1 + w2 + w3 + w4)
tw      = 1 + w1 + w2 + w3 + w4
result  = blurred / tw = occlusion
```

The blur had **zero effect** — it only wasted GPU cycles.

### Fix

Removed the inline blur code and added a `TODO` comment. A proper bilateral
blur requires a **separate pass** (or ping-pong buffer) where the AO texture
is bound as an input and sampled at neighbor UVs.

```wgsl
// TODO: Add a separate AO blur pass. Inline bilateral blur was removed
// because a fragment shader cannot sample its own output texture.
```

---

## 3. Sky Pixel Contamination in Blur (Historical)

### Problem

The removed blur code sampled `gbuffer_normal` neighbors without checking for
sky pixels:

```wgsl
let nn = textureSample(gbuffer_normal, gbuffer_sampler, nuv);
let nd = dot(view_N, normalize(...));
```

GBuffer normal clear value is `(0,0,0)`. Decoded: `(-1,-1,-1)`. `normalize()`
is well-defined for this vector, so no NaN occurred, but the dot product
produced a bogus weight for sky regions, bleeding incorrect occlusion values
into screen edges.

### Fix

Addressed by removing the inline blur. Any future blur pass must guard
against sky pixels before decoding the normal:

```wgsl
if (nn.r == 0.0 && nn.g == 0.0 && nn.b == 0.0) {
    // skip or weight = 0
}
```

---

## 4. Coordinate System Notes

### GBuffer Output

| Texture | Format | Encoding |
|---------|--------|----------|
| Position | `Rgba16Float` | `world_pos` (xyz), w=1.0 |
| Normal | `Rgba16Float` | `world_normal * 0.5 + 0.5`, w=1.0 |

### SSAO Pipeline

1. Read GBuffer position & normal
2. Decode normal: `norm.xyz * 2.0 - 1.0`
3. Transform to view space for depth comparison
4. Build TBN in **world space** (Gram-Schmidt on random vector)
5. Offset sample in world space, reproject to screen UV
6. Read GBuffer at reprojected UV, compare view-space Z

### Why World-Space TBN?

The original LearnOpenGL tutorial builds TBN in view space. Our implementation
builds it in world space because the GBuffer stores world-space position and
normal. Both approaches are valid as long as the sample offset and reprojection
use consistent spaces.

---

## Remaining Work

- [ ] **Add a separate AO blur pass** (e.g. `AOBlurPass` or a generic
  `BilateralBlurPass`) to reduce 16-sample noise.
- [ ] **Tune kernel radius and bias** based on scene scale. Current values
  (`radius=0.5`, `bias=0.025`) are hardcoded and may not fit all scenes.
- [ ] **Consider a noise/blue-noise texture** instead of `hash2()` for more
  stable per-pixel rotation.
