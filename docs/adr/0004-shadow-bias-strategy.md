# ADR-0004: Shadow Bias Strategy

## Status

Accepted

## Context

Shadow mapping with a directional light + orthographic projection in a ~40-unit
scene. Two candidate approaches for preventing shadow acne:

| Approach | Mechanism | Stage |
|----------|-----------|-------|
| **Software** (slope-scale) | `ref_depth = light_ndc.z - bias` in lighting shader | Lighting pass |
| **Hardware** (`DepthBiasState`) | GPU rasterizer offsets depth before writing shadow map | Shadow pass |

We tested both.

### Software approach

Formula from [OpenGL Tutorial 16](https://www.opengl-tutorial.org/intermediate-tutorials/tutorial-16-shadow-mapping/):

```wgsl
let cos_theta = saturate(NdotL);
let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
let slope_bias = base * sin_theta / max(cos_theta, 0.001);
let bias = min(slope_bias, base * 10.0);
let ref_depth = light_ndc.z - bias;
```

- `base = 0.005` (NDC units)
- Grazing surfaces (NdotL → 0): bias clamped to `base × 10 = 0.05`
- Front-facing surfaces (NdotL → 1): bias → 0
- Result: smooth, no acne, no peter panning

### Hardware approach

```rust
DepthBiasState { constant: 0, slope_scale: 2.0, clamp: 0.0 }
```

Result: both acne AND peter panning appeared simultaneously with any
reasonable `slope_scale`. The issue is that `Depth32Float`'s minimal
representable value (~1.2e-7) makes `slope_scale` too coarse for our
projection scale — 1.0 is too much, 0.5 is not enough. There is no usable
middle ground.

## Decision

**Software slope-scale bias in the lighting shader. Hardware `DepthBiasState` kept at zero.**

Rationale:
1. Direct control over NDC-space bias value — tuned to our orthographic projection parameters (near=0.01, far=40.0)
2. Formula is self-documenting in the shader — AI can read and adjust
3. Hardware bias on `Depth32Float` lacks sufficient precision at our scene scale
4. The `tan(acos(NdotL))` formula is well-understood and matches the OpenGL tutorial reference

### What we also decided NOT to use

- **World-space normal offset** (`world_pos + N * bias`): ineffective for vertical surfaces under a top-down light — offset is perpendicular to depth direction, no effect on shadow comparison.
- **Front-face culling** (`CullMode::Front`): shadow map stores back-face depths, causing light leaking through the gap between front and back faces. Only viable with thick geometry and additional bias compensation.

## Consequences

- **Positive**: Predictable bias behavior. Tuning is one scalar (`shadow_normal_bias` uniform, currently 0.005).
- **Negative**: Per-pixel `sqrt`, division, and `min` in lighting shader (small cost on modern GPUs).
- **Neutral**: If projection parameters change significantly (e.g., near/far for a much larger scene), the base value 0.005 may need adjustment. This is inherent to any NDC-space bias.
