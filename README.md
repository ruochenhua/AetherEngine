# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

A modern rendering engine built with **Rust** and **wgpu**, designed for learning real-time graphics from Deferred PBR to ray tracing.

## 🌟 Features

- **Modern Architecture**: ECS (hecs) + RenderGraph-driven pipeline
- **Cross-Platform**: wgpu automatically targets Vulkan/Metal/DX12
- **Deferred Shading**: G-Buffer-based Blinn-Phong PBR with debug visualisation
- **UE-style Fly Camera**: Right-click fly mode, WASD + QE movement, scroll speed
- **Debug Tools**: World grid, RGB axis gizmo, per-component lighting debug
- **Extensible**: Add new RenderPasses without touching existing code
- **AI-Friendly**: Modular design, each module fits in a single AI context window

## 🚀 Quick Start

```bash
# Clone
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# Build
cargo build

# Launcher (recommended entry point)
cargo run -p aether-launcher

# Or run individual examples
cargo run --example 01_triangle
cargo run --example 02_deferred
cargo run --example 03_gltf_scene
```

## 🎮 Controls (02_deferred)

| Input | Action |
|-------|--------|
| `Right Mouse` | Toggle fly mode |
| `W A S D` | Move forward / left / back / right |
| `Q` / `E` | Move down / up (world space) |
| `Mouse` | Look around (fly mode) |
| `Scroll` | Adjust movement speed |
| `0` – `5` | Lighting debug: Full / Ambient / Diffuse / Specular / Normals / NdotL |
| `Esc` | Return to launcher menu |

## 📁 Project Structure

```
├── Cargo.toml
├── crates/
│   ├── aether-engine/          # Main engine crate
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs            # Standalone app entry
│   │       ├── ecs/              # ECS (hecs wrapper)
│   │       ├── scene/            # Scene loading/serialization
│   │       ├── asset/            # Asset management
│   │       ├── renderer/         # Rendering core
│   │       │   ├── graph.rs      # RenderGraph
│   │       │   ├── context.rs    # wgpu context + GBuffer
│   │       │   ├── camera.rs     # FlyCamera + OrbitCamera
│   │       │   └── passes/
│   │       │       ├── gbuffer.rs   # G-Buffer (MRT)
│   │       │       ├── lighting.rs  # Deferred lighting
│   │       │       └── debug.rs     # Line rendering (grid, gizmo)
│   │       ├── physics/          # Physics (reserved)
│   │       ├── math.rs
│   │       ├── input.rs
│   │       └── examples/         # Example implementations
│   └── aether-launcher/         # Unified launcher binary
├── assets/
│   ├── scenes/                   # .ron scene files
│   ├── shaders/                  # .wgsl shaders
│   ├── meshes/                   # GLTF models
│   └── textures/                 # Textures
└── docs/
    └── adr/                      # Architectural decision records
```

## 🏗️ Architecture

### Render Pipeline

```
Launcher (winit event loop)
  └── Example (trait)
        ├── update(dt, input)     # Camera, input, logic
        ├── prepare()             # GPU uploads
        └── render(encoder)       # Command recording
              ├── GBufferPass     # → position, normal, albedo, material
              ├── LightingPass    # → fullscreen quad, Blinn-Phong
              └── DebugLinePass   # → grid, gizmo (depth-tested)
```

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| ECS Library | `hecs` | Minimal API, AI-friendly, no macro magic |
| Render API | `wgpu` | Single backend, auto-adapts to Vulkan/Metal/DX12 |
| Shader Language | WGSL | Unified, no pre-compilation scripts |
| Scene Format | RON | Rust-native, type-safe, AI-friendly |
| UI | `egui` | Immediate mode, easy debugging panels |

## 📅 Roadmap

| Phase | Features | Status |
|-------|----------|--------|
| **Phase 0** | Window, triangle, egui, launcher | ✅ Complete |
| **Phase 1** | Deferred PBR, fly camera, debug tools | 🚧 In Progress |
| **Phase 2** | Shadows, IBL, scene YAML | 🔲 Planned |
| **Phase 3** | SSR + SSAO + Post-Process | 🔲 Planned |
| **Phase 4** | Terrain + Atmosphere + Water + Clouds | 🔲 Planned |
| **Phase 5** | Ray Tracing (Compute + Hybrid) | 🔲 Planned |

## 🤝 Contributing

This is a personal learning project. The codebase is designed to be AI-collaboration friendly:

- Each module is self-contained (< 500 LOC)
- Clear trait interfaces (`RenderPass`, `System`, `Asset`)
- No complex generic constraints
- Comprehensive doc comments

## 📜 License

MIT OR Apache-2.0

---

*Aether Engine is the spiritual successor to [KongEngine](https://github.com/ruochenhua/KongEngine), rebuilt with modern architecture and Rust.*
