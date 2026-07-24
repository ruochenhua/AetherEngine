# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

A modern rendering engine built with **Rust** and **wgpu**, designed for learning real-time graphics from Deferred PBR to ray tracing.

> **This is an AI-first codebase.** Every architectural decision — module boundaries, interface design, test strategy, and contribution workflow — is optimized for AI agents as the primary developers, with humans in the review loop. See [AI-First Design](#-ai-first-design) below.

## 🌟 Features

- **Modern Architecture**: ECS (hecs) + type-safe pass scheduling (PipelineBuilder / Scheduler)
- **Cross-Platform**: wgpu automatically targets Vulkan/Metal/DX12
- **Deferred Shading**: G-Buffer-based Cook-Torrance PBR (GGX NDF + Smith G + Schlick Fresnel)
- **Image-Based Lighting**: Diffuse irradiance + specular prefiltered cubemap + BRDF LUT
- **Skybox**: High-resolution environment cubemap rendering
- **UE-style Fly Camera**: Alt + left-drag to look, WASD + QE movement, scroll speed
- **Debug Tools**: World grid, RGB axis gizmo, per-component lighting + IBL debug
- **Scene Editor**: Pick objects by click, transform gizmo (translate / rotate / scale), hierarchy panel, inspector (position / rotation / scale / material), undo/redo, delete, open/import/save scenes (RON), fullscreen viewport toggle
- **AI-First**: Single-responsibility modules (~600 LOC of pure logic is healthy, inline shaders excluded); adding a pass = one file + one registration line
- **Test-Driven**: Red-green-refactor on every change; build-time catch for resource wiring errors

## 🚀 Quick Start

```bash
# Clone
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# Build
cargo build

# Launcher (recommended entry point)
cargo run -p aether-launcher
```

## 🎮 Controls

| Input | Action |
|-------|--------|
| `Alt + Left Mouse + Drag` | Look around (yaw / pitch) |
| `Left Click` | Pick object in viewport (`Shift`/`Ctrl` + Click adds to the selection) |
| `Left Drag` on a gizmo handle | Translate / rotate / scale the selected entity |
| `Delete` | Delete selected entities |
| `W A S D` | Move forward / left / back / right |
| `Q` / `E` | Move down / up (world space) |
| `Scroll` | Adjust movement speed |
| `0` – `9` | Lighting debug: Full / Ambient / Diffuse / Specular / Normals / NdotL / Shadow / Direct / IBL / Alpha(P) |
| `F1` – `F5` | Extra debug views: Alpha(N) / NDC / EnvFix / VDir / SSAO |
| `F6` | Cycle SSR debug mode |
| `F7` | CSM shadow debug view |
| `⛶ Full Screen` | Toggle fullscreen viewport (hides side panels) |

> **Note:** Debug hotkeys (`0`–`9`, `F1`–`F7`) are automatically blocked when an egui input field has keyboard focus, preventing accidental mode switches while editing values.

## 🤖 AI-First Design

Aether Engine is not just built *with* AI — it is built **for** AI. Every design choice is evaluated through the lens of an AI agent's capabilities and limitations.

### Core Principles

| Principle | What it means |
|-----------|---------------|
| **Single-responsibility modules** | One file, one responsibility. ~600 LOC of pure logic is healthy (inline shaders don't count); split past ~800 LOC only when a second independent responsibility emerges. |
| **Declarative over imperative** | Pipeline structure is declared via `PipelineBuilder::add(pass)`, not hidden in a 600-line render loop. |
| **Type-safe wiring** | `ResHandle<GPosition>` vs `ResHandle<GNormal>` — the compiler catches resource mix-ups before render time. |
| **Build-time failure** | Missing resource producer → panic at `build()`. TDD first cycle catches it. No runtime black-screen debugging. |
| **Template-driven creation** | Adding a new pass = copy `passes/template.rs` → fill in signature + shader → register one line in `build_pipeline()`. |
| **Flat dependency graph** | No deep inheritance. Passes depend on a single `Pass` trait. Systems depend on a single `System` trait. |
| **Human in review, AI in writing** | AI writes PRs; human reviews for architectural fit and visual correctness. Tests prove the code works. |

### Module Dependency Graph

```
main.rs (~7 lines) ──→ app.rs (launcher orchestration: event loop + editor UI)
  │
  ├── pipeline::build_pipeline() ──→ PipelineBuilder ──→ Scheduler ──→ [passes in topological order]
  ├── SceneLoader ──→ RON scene → ECS World entities + LightingUniforms
  ├── Extract ──→ per-frame RenderBatch from the ECS World
  ├── FlyCamera ──→ view/proj matrices
  ├── InputManager ──→ keyboard/mouse state
  └── egui ──→ editor UI + debug overlay
```

**Dependency rules:**
- The launcher (`main.rs` + `app.rs`) depends on all public APIs — but only through thin orchestration
- Pass modules only depend on `Pass` trait + `wgpu` + their own shaders
- Adding a pass: create `passes/new_pass.rs` → add one line in `build_pipeline()` → add one setter call in main loop
- Scheduler, PipelineBuilder, ResourceTable are **write-once** infrastructure

### How AI Adds a New Pass (e.g. SSAO)

```
1. Copy     passes/template.rs       → passes/ssao.rs
2. Fill in  signature()              → reads: GPosition, GNormal; writes: AOTexture
3. Fill in  init() / resolve()       → create pipeline + bind groups
4. Fill in  execute()                → record commands
5. Register builder.add(SSAOPass::init(device))  ← 1 line
6. Add      ssao_pass.set_config(...)            ← 1 line in main loop
7. Run tests → fix build-time errors → PR
```

**Files touched: 2** (new pass file, main.rs). **Files to review: 1** (the new pass).

### Development Conventions

- **Tests first**: write the failing test → write minimal code to pass → refactor. Never write implementation before tests.
- **Public interface testing**: tests verify behavior through public APIs. Never test private functions.
- **Build-time errors > runtime errors**: prefer types that make invalid states unrepresentable.
- **No implicit coupling**: if pass B depends on pass A's output, it must declare it in `signature()`.
- **Shaders inline**: WGSL lives inside the Rust pass file. One file = complete context for AI.

## 📁 Project Structure

```
├── Cargo.toml
├── crates/
│   ├── aether-engine/          # Engine library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ecs/              # ECS (hecs wrapper)
│   │       ├── scene/            # Scene loading + RON (de)serialization
│   │       ├── asset/            # Asset management + mesh registry
│   │       ├── terrain/          # Chunked-LOD terrain geometry + material
│   │       ├── clouds/           # Procedural noise textures for volumetric clouds
│   │       ├── renderer/         # Rendering core
│   │       │   ├── pass.rs            # Pass trait (signature / init / resolve / execute)
│   │       │   ├── pipeline_builder.rs# PipelineBuilder + topological sort
│   │       │   ├── scheduler.rs       # Scheduler (execution + resize rebuild)
│   │       │   ├── resource.rs        # ResHandle<T> + resource tags
│   │       │   ├── resource_table.rs  # ResourceTable (transient textures)
│   │       │   ├── frame.rs           # RenderFrame per-frame data
│   │       │   ├── extract.rs         # ECS → RenderBatch extraction
│   │       │   ├── context.rs         # wgpu context + RenderContext
│   │       │   ├── camera.rs          # FlyCamera
│   │       │   ├── ibl/               # IBL precomputation + skybox
│   │       │   └── passes/            # All render passes (see Render Pipeline below)
│   │       │       ├── template.rs          # AI copy-paste template
│   │       │       ├── shadow.rs            # Cascaded shadow maps
│   │       │       ├── gbuffer.rs           # G-Buffer (MRT)
│   │       │       ├── terrain/             # Terrain into G-Buffer (optional)
│   │       │       ├── ssao.rs, ao_blur.rs  # SSAO + position-aware blur (half-res)
│   │       │       ├── lighting/            # Deferred lighting
│   │       │       ├── atmosphere.rs        # Analytic sky
│   │       │       ├── volumetric_cloud/    # Ray-marched volumetric clouds
│   │       │       ├── ssr/                 # Screen-space reflections
│   │       │       ├── god_ray.rs           # God rays
│   │       │       ├── water_reflection.rs  # Planar water reflection
│   │       │       ├── water/               # Forward water pass
│   │       │       ├── composite.rs         # Merge opaque + water + clouds + god rays
│   │       │       ├── bloom/               # Bloom mip chain
│   │       │       ├── tone_mapping.rs      # HDR → LDR
│   │       │       ├── fxaa.rs              # FXAA to swapchain
│   │       │       └── debug.rs             # Line rendering (grid, gizmo)
│   │       ├── physics/          # Physics (reserved)
│   │       ├── math.rs
│   │       └── input.rs
│   └── aether-launcher/         # Launcher binary (thin orchestration)
│       └── src/
│           ├── main.rs           # Entry point (~7 lines)
│           ├── app.rs + app/     # Event loop, UI, editor interaction
│           ├── inspector/        # Inspector panel
│           └── pipeline.rs       # build_pipeline() + screenshot helpers
├── scenes/                      # .ron scene files
├── assets/                      # Meshes, textures, shaders
└── docs/
    └── adr/                     # Architectural Decision Records
```

## 🏗️ Architecture

### Render Pipeline

```
build_pipeline() (crates/aether-launcher/src/pipeline.rs) → PipelineBuilder
  ├── ShadowPass           → writes: ShadowDepth (cascade array)
  ├── GBufferPass          → writes: GPosition, GNormal, GAlbedo, GMaterial, GDepth
  ├── TerrainPass          → writes: same G-buffer targets (terrain merged into the G-buffer)
  │                          optional, when scene has terrain
  ├── SSAOPass             → reads: GDepth, GNormal → writes: AOTexture (half-res)
  ├── AOBlurPass           → reads: AOTexture, GPosition → writes: AOTextureBlurred (half-res)
  │                          (SSAO + AOBlur are runtime-toggleable via ssao_enabled)
  ├── LightingPass         → reads: GPosition, GNormal, GAlbedo, GMaterial, ShadowDepth, AOTextureBlurred
  │                        → writes: SceneColor (HDR)
  ├── AtmospherePass       → reads: GDepth → writes: SceneColor (sky drawn after lighting)
  │                          optional, when scene has atmosphere
  ├── VolumetricCloudPass  → reads: GDepth → writes: CloudColor
  │                          optional, when scene has clouds
  ├── SSRPass              → reads: GPosition, GNormal, GMaterial, GDepth, SceneColor
  │                        → writes: SsrTraceResult (half-res) + ReflectionTexture
  ├── GodRayPass           → reads: GDepth → writes: GodRayColor
  │                          optional, when scene has god rays
  ├── WaterReflectionPass  → writes: WaterReflectionColor, WaterReflectionDepth (planar reflection)
  │                          optional, when scene has water with reflection enabled
  ├── WaterPass            → reads: SceneColor, ReflectionTexture, WaterReflectionColor, GDepth
  │                        → writes: WaterColor     optional, when scene has water
  ├── CompositePass        → reads: SceneColor, ReflectionTexture, WaterColor, CloudColor, GodRayColor, G-buffer
  │                        → writes: PostProcessInput
  ├── BloomPass            → reads: PostProcessInput → writes: BloomResult
  ├── ToneMappingPass      → reads: BloomResult → writes: FxaaInput (HDR → LDR)
  ├── FXAAPass             → reads: FxaaInput → writes: Swapchain
  └── DebugLinePass        → reads: GDepth → writes: Swapchain (grid/gizmo lines over the final image)
```

Resource wiring is type-checked at build time. The Scheduler derives execution order by topologically sorting these signatures — passes with no pending dependencies (e.g. WaterReflectionPass, VolumetricCloudPass, GodRayPass) may execute earlier than their registration position. Optional passes are skipped per frame via `should_run()`; TerrainPass is only registered when the scene contains terrain (ADR-0010). DebugLinePass is registered last and, as the final sequential writer of `Swapchain`, always executes last.

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Pass Scheduling | PipelineBuilder + Scheduler | Declaration over imperative loop — AI understands structure without reading main.rs |
| Resource Wiring | `ResHandle<T>` type tags | Compile-time safety — AI can't confuse texture semantics |
| ECS Library | `hecs` | Minimal API, AI-friendly, no macro magic |
| Render API | `wgpu` | Single backend, auto-adapts to Vulkan/Metal/DX12 |
| Shader Language | WGSL | Unified, inline — complete context for AI |
| Scene Format | RON | Rust-native, type-safe, AI generates clean RON |
| UI | `egui` | Immediate mode, easy debugging panels |
| Test Strategy | TDD + public-interface only | AI writes test first, gets compiler feedback, refactors safely |

## 📅 Roadmap

| Phase | Features | Status |
|-------|----------|--------|
| **Phase 0** | Window, triangle, egui, launcher | ✅ Complete |
| **Phase 1** | Deferred PBR, fly camera, debug tools, type-safe scheduler, shadow mapping, IBL + skybox | ✅ Complete |
| **Phase 2** | Screen-space effects (SSAO, SSR) | ✅ Complete |
| **Phase 3** | ECS runtime, ray picking, transform gizmo, editor UI shell, scene save/load, undo/redo, delete | ✅ Complete |
| **Phase 4** | Post-process chain, tone mapping, Bloom, FXAA, GPU Instancing | ✅ Complete |
| **Phase 5** | Terrain + Atmosphere + Water + Volumetric Clouds + God Rays | ✅ Complete |
| **Phase 6** | Polish & engineering health (tests, shader error handling, perf, docs alignment) | 🔲 Current |

> **Note:** Ray tracing (Compute Path Tracer / Hybrid RT / Denoising) is postponed — it will be re-planned after the polish phase.

## 📜 License

MIT OR Apache-2.0

---

*Aether Engine is the spiritual successor to KongEngine, rebuilt with AI-first architecture.*
