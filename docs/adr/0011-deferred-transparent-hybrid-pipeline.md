# ADR-0011: Deferred + Transparent Forward Hybrid Pipeline

## Status

Accepted

## Context

AetherEngine's rendering pipeline is fully deferred as of Phase 4:

```
Shadow → GBuffer → SSAO → Lighting → SSR → Composite → Bloom → ToneMap → FXAA → DebugLine
```

Phase 5 adds water, which is transparent and requires refraction and Fresnel
reflections. Transparent surfaces cannot be correctly rendered with the
existing opaque deferred pipeline because:

1. They need per-pixel sorting or painter's-algorithm ordering.
2. They need to sample the already-lit scene color for refraction.
3. They need view-dependent Fresnel blending between reflection and refraction.
4. They often need their own vertex animation (Gerstner waves).

This raises the question of whether AetherEngine should remain purely deferred,
add a generic transparent pass, or implement water as a special case.

## Decision

Introduce a **transparent forward pass** that executes after the deferred
opaque pipeline has produced a lit scene color. Water is the first user of this
pass. The pipeline order becomes:

```
Shadow → GBuffer → SSAO → Lighting → SSR → WaterPass → Composite → Bloom → ToneMap → FXAA → DebugLine
```

### Key design points

1. **Opaque stays deferred**
   - Terrain, meshes, and other opaque objects continue to use GBuffer +
     Lighting.
   - This preserves all existing work and keeps the common case fast.

2. **WaterPass is a forward pass over the lit scene**
   - `WaterPass` runs after `SSRPass` and before `CompositePass`.
   - It renders a water mesh displaced by Gerstner waves in the vertex shader.
   - In the fragment shader it computes:
     - Reflection via the existing SSR result or a simple sky reflection.
     - Refraction by refracting the view ray, sampling the lit scene color
       texture at the hit point, and masking by water depth.
     - Fresnel blending between reflection and refraction.
     - Optional normal map perturbation for small-scale surface detail.

3. **Reflection source**
   - Phase 5 uses screen-space reflections (SSR) already produced by `SSRPass`.
   - This avoids planar reflection render-to-texture, which doubles geometry
     cost and requires special handling for terrain.
   - Where SSR misses (screen edges, off-screen objects), the pass falls back
     to the sky/atmosphere color.

4. **Refraction source**
   - The pass reads the lit scene color produced by `LightingPass` (before
     bloom/tone mapping).
   - It uses GBuffer depth and the water surface normal to compute a refracted
     ray, then samples the scene color at the intersection point.
   - Deep water tint is applied based on the distance between the water surface
     and the underwater hit.

5. **No generic transparent system in Phase 5**
   - `WaterPass` is a dedicated pass, not a generic "transparent object"
     renderer.
   - Generic transparent sorting, multiple transparent layers, and blended
     particle systems are out of scope for Phase 5.
   - If future phases need glass, particles, or other transparent objects, the
     water pass can be generalized into a `TransparentPass`.

## Considered Options

- **Render water as opaque GBuffer object**:
  rejected — water cannot be opaque. Without transparency and refraction it
  looks like colored plastic and cannot show the underwater scene.

- **Deferred water with special GBuffer blending**:
  rejected — deferred pipelines can support limited transparency (e.g., depth
  peeling), but the complexity is high and the results are worse than a forward
  pass for a single prominent surface like water.

- **Planar reflection render-to-texture**:
  rejected — it produces high-quality reflections but requires rendering the
  scene again from a mirrored camera, which is expensive and complicated when
  the reflected scene includes terrain and moving objects. SSR is a better
  Phase 5 trade-off.

- **Generic `TransparentPass` from the start**:
  rejected — sorting transparent primitives, handling multiple transparent
  materials, and editor integration add significant scope. A dedicated
  `WaterPass` delivers the Phase 5 requirement with less risk and can be
  generalized later.

- **Post-process water without geometry**:
  rejected — a screen-space water effect cannot produce real shoreline
  interaction, wave silhouettes, or camera-parallax response.

## Consequences

- **Pros**:
  - Water gets correct transparency, refraction, and Fresnel reflection.
  - The deferred opaque pipeline remains untouched.
  - SSR and the lit scene color are reused, minimizing new resources.
  - The design creates a clear extension point for future transparent effects.

- **Cons**:
  - Water lighting differs from the deferred PBR path; it must compute its own
    diffuse/specular response or accept an approximation.
  - SSR quality limits reflection quality (no off-screen reflections).
  - Adding more transparent objects in the future will require redesigning
    `WaterPass` into a generic `TransparentPass` with sorting.
