# ADR-0005: Unified Cook-Torrance BRDF for Direct Lighting and IBL

## Status

Accepted

## Context

The rendering pipeline uses two different BRDF models:

| Component | Model | NDF | Geometry | Fresnel |
|-----------|-------|-----|----------|---------|
| Direct lighting | Blinn-Phong | Phong lobe (pow) | None | Simplified `mix(0.04, albedo, metallic)` |
| IBL (specular) | Cook-Torrance via split-sum | GGX (Trowbridge-Reitz) | Smith + Schlick-GGX (baked in BRDF LUT) | Schlick (baked in BRDF LUT) |

This mismatch causes:

1. **Inconsistent specular shape**: Direct lighting produces a symmetric Phong lobe; IBL produces the longer-tail GGX lobe. Rough metals show noticeably different specular highlights from the two sources.
2. **Inconsistent diffuse scaling**: IBL diffuse uses `kD = (1.0 - metallic) * 0.96` (hardcoded F0=0.04); direct lighting diffuse uses unscaled `albedo`. Energy conservation is violated — combined diffuse + specular may exceed incident energy, and metals get too much diffuse IBL.
3. **Duplicate F0 computation**: F0 is computed once for Blinn-Phong specular color, and again differently for the roughness-modified Fresnel in IBL. Two Fresnel evaluations with subtly different parameters.

The IBL precomputation (`crates/aether-engine/src/renderer/ibl.rs`) already bakes GGX importance-sampled prefiltering and a GGX/Smith BRDF LUT. Fixing the direct lighting side to use the same BRDF is a shader-only change — no IBL precomputation pipeline changes needed.

## Decision

**Unify all lighting onto the Cook-Torrance BRDF (GGX NDF + Smith geometry + Schlick Fresnel).**

The lighting shader (`passes/lighting.rs`) now uses the same BRDF functions as the IBL split-sum approximation:

```
distribution_ggx(NdotH, roughness)      → NDF (Trowbridge-Reitz)
geometry_smith(NdotV, NdotL, roughness) → G (Schlick-GGX with k = (r+1)²/8)
fresnel_schlick(VdotH, F0)              → F (Schlick)
```

Direct lighting specular:

```
specular = (D * G * F) / (4 * NdotV * NdotL + 0.0001) * NdotL * radiance
```

Direct lighting diffuse with energy-conserving kD:

```
F = fresnel_schlick(VdotH, F0)
kD = (1.0 - F) * (1.0 - metallic)
diffuse = kD * albedo / PI * NdotL * radiance
```

IBL uses the same `F` (Fresnel) and `kD` as direct lighting — computed once:

```
diffuse_ibl  = kD * albedo * irradiance
specular_ibl = prefiltered_color * (F * envBRDF.x + envBRDF.y)
```

The IBL precomputation pipeline is unchanged — the BRDF LUT already uses matching geometry (`k = (r+1)²/8`).

### Also fixed

- **Sky pixels**: Previously returned hardcoded gray `(0.05, 0.05, 0.05)`. Now samples the prefiltered environment cubemap at roughness 0 using a view ray reconstructed from screen UV via `inv_view_proj`. Added `inv_view_proj: mat4x4<f32>` to `LightingUniforms`.
- **No multiple-scattering compensation**: Deferred to a future ADR (requires additional compute shader or analytic term — Kulla-Conty or Filament-style `F_avg`).

## Consequences

### Positive

- **Visual consistency**: Direct light specular and IBL specular use the same microfacet distribution. Roughness slider produces the same lobe shape in both components.
- **Energy conservation**: Diffuse and specular share a single Fresnel term — `kD + kS ≤ 1` per incident direction. No more double-counting.
- **Correct Fresnel for metals**: `kD` is derived from actual `F(VdotH, F0)`, not a hardcoded 0.04 approximation. Metals correctly suppress diffuse.
- **Simpler code**: No roughness-modified Fresnel hack for IBL. F0, F, and kD computed once.

### Negative

- **Appearance change**: Existing scenes will look different. The Blinn-Phong `shininess = mix(8, 128, 1-roughness)` maps to a different visual brightness than GGX at the same roughness. Light intensities in scene RON files may need tuning.
- **Diffuse dimmer by factor of π**: The Lambertian `albedo / PI` term is now correctly applied. Scenes relying on the previous over-bright diffuse will need light intensity adjustment (e.g., multiply directional light intensity by π).
- **Slightly more ALU**: 2 square roots and a pow() replaced with GGX denominator + Schlick-GGX evaluation (negligible on modern GPUs).
