# ADR-0003: Type-safe pass scheduling with PipelineBuilder

## Status

Proposed

## Context

The Launcher's rendering loop (main.rs, ~665 lines) hardcodes the pass execution
order: GBuffer → Lighting → Debug → egui.  Adding a new pass (e.g. SSAO)
requires editing 3–5 locations across main.rs, breaking the AI-friendly
principle that each pass should be a self-contained module.  The existing
`RenderGraph` and `RenderPass` trait are stubs — passes bypass them entirely
and use ad-hoc `execute()` signatures.

The goal is an architecture where AI agents can:
1. Add a new pass by writing one file and adding one registration line
2. Understand the pipeline structure through declaration, not by reading
   imperative render loops
3. Catch resource wiring errors at build time (missing producers, type
   mismatches) rather than at runtime

## Decision

Introduce a **PipelineBuilder → Scheduler** infrastructure that replaces the
hardcoded render loop.  All passes implement a uniform `Pass` trait with
three phases (`init` → `resolve` → `execute`) and declare their resource
dependencies as type-safe signatures.

Key design points:

1. **`ResHandle<T>`** — zero-size type tags (`GPosition`, `GNormal`,
   `Swapchain`, …) make resource handles type-safe.  The builder connects
   pass outputs to downstream pass inputs by matching `(type, name)` pairs.

2. **`Pass` trait** — every pass exposes a `signature()` listing its reads
   and writes, an `init()` phase for GPU resources unrelated to textures
   (pipelines, shaders, uniform buffers), and a `resolve()` phase called by
   the builder to create texture-dependent bind groups.

3. **`PipelineBuilder`** — collects passes, their signatures, and their
   `ResHandle<T>` destinations.  On `build()`, topologically sorts the
   dependency graph, allocates transient textures, and calls `resolve()` on
   each pass with the `ResourceTable`.  On `rebuild()`, re-allocates
   resolution-dependent textures and re-resolves.

4. **Per-frame data** — each pass exposes its own typed setters (e.g.
   `set_frame_data()`, `set_uniforms()`) rather than a central `FrameData`
   struct, so adding a pass never requires modifying a shared type.

5. **Error detection** — missing resource producers and dependency cycles are
   detected at `build()` time.  Resource type mismatches are caught by the
   Rust compiler.

## Considered Options

- **Keep current architecture, add pass via editor discipline**: rejected —
  the risk of AI modifying main.rs in 3+ places per pass compounds across
  many passes and phases.
- **String-keyed resource table**: rejected — typos only surface at render
  time.  Type tags give the compiler and AI the same feedback loop.
- **Central `FrameData` struct**: rejected — forces every new pass to modify
  the shared struct, recreating the same friction the scheduler eliminates
  for resource wiring.
- **Declarative graph file (YAML/TOML)**: rejected — adds a second language
  and an indirection that AI must learn.  Builder pattern in Rust is self-
  documenting and compile-checked.
- **Full render graph with automatic barrier insertion and multi-queue
  scheduling**: rejected — Phase 1-3 don't need it, and the complexity would
  outweigh the benefit for a single-queue deferred pipeline.

## Consequences

- **Pros**: single-file pass definitions; build-time resource wiring
  validation; type-safe resource handles; minimal main.rs changes when adding
  passes.
- **Cons**: ~200 lines of scheduler infrastructure that must be understood
  once; `init`/`resolve` two-phase construction reflects wgpu's inherent
  texture-dependency constraint but adds ceremony.
