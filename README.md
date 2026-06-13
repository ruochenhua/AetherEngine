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
- **UE-style Fly Camera**: Left-click drag to look, WASD + QE movement, scroll speed
- **Debug Tools**: World grid, RGB axis gizmo, per-component lighting + IBL debug
- **Scene Editor**: Pick objects by click, transform gizmo (translate / rotate / scale), hierarchy panel, inspector (position / rotation / scale / material), undo/redo, delete, open/import/save scenes (RON), fullscreen viewport toggle
- **AI-First**: Every module fits a single AI context window; adding a pass = one file + one registration line
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
| `Left Mouse + Drag` | Look around |
| `W A S D` | Move forward / left / back / right |
| `Q` / `E` | Move down / up (world space) |
| `Scroll` | Adjust movement speed |
| `0` – `9` | Lighting debug: Full / Ambient / Diffuse / Specular / Normals / NdotL / Shadow / Direct / IBL / Alpha |
| `F1` – `F4` | IBL/Skybox debug: NormalAlpha / NDC / EnvFix / VDir |
| `Alt + Left Drag` | Orbit camera (editor mode) |
| `Left Click` | Pick object in viewport |
| `⛶ Full Screen` | Toggle fullscreen viewport (hides side panels) |

> **Note:** Debug hotkeys (`0`–`9`, `F1`–`F4`) are automatically blocked when an egui input field has keyboard focus, preventing accidental mode switches while editing values.

## 🤖 AI-First Design

Aether Engine is not just built *with* AI — it is built **for** AI. Every design choice is evaluated through the lens of an AI agent's capabilities and limitations.

### Core Principles

| Principle | What it means |
|-----------|---------------|
| **Single-file modules** | Each module < 500 LOC. An AI can read, understand, and regenerate a module in one context window. |
| **Declarative over imperative** | Pipeline structure is declared via `PipelineBuilder::add(pass)`, not hidden in a 600-line render loop. |
| **Type-safe wiring** | `ResHandle<GPosition>` vs `ResHandle<GNormal>` — the compiler catches resource mix-ups before render time. |
| **Build-time failure** | Missing resource producer → panic at `build()`. TDD first cycle catches it. No runtime black-screen debugging. |
| **Template-driven creation** | Adding a new pass = copy `passes/template.rs` → fill in signature + shader → register one line in `build_pipeline()`. |
| **Flat dependency graph** | No deep inheritance. Passes depend on a single `Pass` trait. Systems depend on a single `System` trait. |
| **Human in review, AI in writing** | AI writes PRs; human reviews for architectural fit and visual correctness. Tests prove the code works. |

### Module Dependency Graph

```
main.rs (thin orchestration, ~80 lines)
  │
  ├── PipelineBuilder ──→ Scheduler ──→ [Passes in topological order]
  │     ↑                                    │
  │     └── ShadowPass.init()               │
  │     └── GBufferPass.init()              │
  │     └── LightingPass.init()             │
  │     └── DebugLinePass.init()            │
  │                                          │
  ├── SceneLoader ──→ SceneResources { renderables, lighting }
  ├── FlyCamera ──→ view/proj matrices
  ├── InputManager ──→ keyboard/mouse state
  └── egui ──→ debug overlay
```

**Dependency rules:**
- `main.rs` depends on all public APIs — but only through thin orchestration
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
│   │       ├── scene/            # Scene loading + RON deserialization
│   │       ├── asset/            # Asset management + mesh registry
│   │       ├── renderer/         # Rendering core
│   │       │   ├── pass.rs       # Pass trait (signature / init / resolve / execute)
│   │       │   ├── scheduler.rs  # Scheduler + PipelineBuilder
│   │       │   ├── resource.rs   # ResHandle<T> + ResourceTable
│   │       │   ├── ibl.rs        # IBL precomputation + skybox
│   │       │   ├── camera.rs     # FlyCamera
│   │       │   └── passes/
│   │       │       ├── template.rs  # AI copy-paste template
│   │       │       ├── gbuffer.rs   # G-Buffer (MRT)
│   │       │       ├── lighting.rs  # Deferred lighting
│   │       │       └── debug.rs     # Line rendering (grid, gizmo)
│   │       ├── physics/          # Physics (reserved)
│   │       ├── math.rs
│   │       ├── input.rs
│   │       └── window.rs
│   └── aether-launcher/         # Launcher binary (thin orchestration)
├── scenes/                      # .ron scene files
├── assets/                      # Meshes, textures, shaders
└── docs/
    └── adr/                     # Architectural Decision Records
```

## 🏗️ Architecture

### Render Pipeline

```
PipelineBuilder
  ├── ShadowPass       → writes: ShadowDepth
  ├── GBufferPass      → writes: GPosition, GNormal, GAlbedo, GMaterial, GDepth
  ├── SSAOPass         → reads: GPosition, GNormal  → writes: AOTexture
  ├── LightingPass     → reads: GPosition, GNormal, GAlbedo, GMaterial, ShadowDepth, AOTexture
  │                        writes: Swapchain
  ├── SSRPass          → reads: GPosition, GNormal, GAlbedo, GMaterial → writes: ReflectionTexture
  ├── CompositePass    → composites Lighting + SSR → writes: Swapchain
  └── DebugLinePass    → reads: GDepth  → writes: Swapchain (LoadOp::Load)
```

Resource wiring is type-checked at build time. Execution order is topological.

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
| **Phase 6** | Ray Tracing (Compute + Hybrid) | 🔲 Current |

## 📜 License

MIT OR Apache-2.0

---

*Aether Engine is the spiritual successor to KongEngine, rebuilt with AI-first architecture.*
