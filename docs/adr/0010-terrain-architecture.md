# ADR-0010: Terrain Architecture for Phase 5

## Status

Accepted

## Context

Phase 5 introduces terrain rendering into AetherEngine. Unlike the screen-space
effects in Phase 1–4, terrain is a geometry-heavy, LOD-driven feature that
cannot be expressed as a single shader pass. Several architectural questions
must be resolved before implementation:

1. **Scene integration**: Is terrain a special object in the scene list, a
   scene-level global configuration, or a standalone resource?
2. **Geometry strategy**: wgpu has no hardware tessellation stage. How do we
   generate terrain geometry at varying detail levels?
3. **ECS representation**: Are individual terrain chunks entities, or is the
   terrain a single entity whose chunks are managed internally?
4. **Pipeline integration**: Does terrain reuse the existing Deferred pipeline
   (GBuffer → Lighting) or render with its own lighting?
5. **Optional feature**: Not every scene has terrain. How is the terrain pass
   enabled and disabled without runtime overhead?

The long-term goal is large-world terrain with compute-based tessellation and
streaming (C-level). Phase 5 must choose a stepping-stone design that can grow
into that goal without being rewritten.

## Decision

Terrain is a **scene-level, optional feature** represented by a single
`Terrain` ECS entity. It is rendered by an optional `TerrainPass` that writes
into the existing GBuffer so that Lighting, SSAO, SSR, shadows, and
post-processing apply automatically. The pass internally manages a **Chunked
LOD** grid; chunks are not exposed as editor-manipulable entities.

### Key design points

1. **Scene-level configuration**
   - `SceneDescription` gains a top-level `terrain: Option<TerrainConfig>` field.
   - `TerrainConfig` contains a `TerrainSource` enum (`Heightmap(path)` or
     `Procedural(seed, params)`), splat map, material scale, and LOD parameters.
   - This keeps terrain semantically distinct from user-placed objects and
     leaves room for future global settings (`atmosphere`, `water`, `clouds`).

2. **TerrainSource abstraction**
   - `TerrainSource` decouples height data acquisition from rendering.
   - Phase 5 implements `Heightmap` and a simple `Procedural` source.
   - Future sources (runtime generated, streaming tiles) can be added without
     changing the pass.

3. **Chunked LOD geometry**
   - Phase 5 uses a fixed grid of terrain chunks, each with multiple
     pre-generated LOD meshes.
   - LOD selection and frustum culling happen inside `TerrainPass` based on
     chunk distance to the camera and chunk AABB.
   - Skirts or transition skirts handle LOD cracks.
   - This is the B-level target. The long-term C-level target is compute-based
     tessellation + Indirect Draw; Chunked LOD is the natural pathway because
     the chunk management system can be reused for tile streaming.

4. **Single `Terrain` entity, internal chunk management**
   - `SceneLoader` spawns one entity with a `Terrain` component.
   - `TerrainPass` owns the chunk meshes, LOD state, and culling logic.
   - Chunks do not appear in the editor hierarchy. The hierarchy shows a single
     collapsible "Terrain" node.
   - This avoids polluting ECS with hundreds of `TerrainChunk` entities and
     keeps editor interactions (selection, undo, inspector) focused on the
     terrain as a whole.

5. **GBuffer integration**
   - `TerrainPass` writes the same GBuffer resources as `GBufferPass`:
     `GPosition`, `GNormal`, `GAlbedo`, `GMaterial`, `GDepth`.
   - `LightingPass` and downstream passes require no changes because they
     already consume these resources.
   - This gives terrain automatic access to shadows, SSAO, SSR, bloom, and
     tone mapping.

6. **Optional pass registration**
   - `TerrainPass` is registered in `PipelineBuilder` only when the scene has
     `terrain.is_some()`.
   - Enabling or disabling terrain triggers a `Scheduler::rebuild()`.
   - This matches the existing rebuild path used for window resize and avoids
     carrying a no-op pass in non-terrain scenes.

## Considered Options

- **Terrain as a special object in `SceneDescription.objects`**:
  rejected — terrain is not a user-placed mesh object. Mixing it with objects
  pollutes the object schema and makes RON files confusing.

- **Dense uniform grid + vertex displacement (no LOD)**:
  rejected — a fixed-resolution grid cannot support roamable terrain. It is
  acceptable as an A-level prototype but does not meet the Phase 5 B-level
  goal.

- **Compute tessellation + Indirect Draw for Phase 5**:
  rejected — this is the long-term C-level target. It requires compute
  infrastructure, indirect draw support, and complex debugging that is
  excessive for the first terrain implementation.

- **Each chunk as an ECS entity with `MeshHandle`**:
  rejected — chunks are rendering detail, not scene objects. Exposing them in
  ECS would clutter the editor hierarchy, complicate picking, and force LOD
  mutations to go through the entity system.

- **Terrain as a forward-rendered special pass with custom lighting**:
  rejected — it would duplicate lighting, shadow, SSAO, and SSR logic already
  implemented in the deferred pipeline. Writing into the GBuffer reuses the
  existing work.

- **Always register `TerrainPass` and early-exit when no terrain**:
  rejected — it violates the optional-pass principle and adds per-frame
  overhead to every non-terrain scene.

## Consequences

- **Pros**:
  - Terrain integrates cleanly with the existing deferred pipeline.
  - The design can evolve from B-level Chunked LOD to C-level streaming by
    reusing chunk management.
  - Editor UX remains simple: one terrain node, not hundreds of chunks.
  - No runtime overhead in scenes that do not use terrain.

- **Cons**:
  - `TerrainPass` must implement its own culling and LOD selection instead of
    reusing a generic system.
  - Adding terrain requires rebuilding the scheduler, which adds a brief
    stutter when enabling terrain in the editor.
  - The GBuffer format constrains the terrain material output; exotic terrain
    effects would need to extend the GBuffer.
