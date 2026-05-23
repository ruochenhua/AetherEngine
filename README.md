# Aether Engine

A modern rendering engine built with **Rust** and **wgpu**, designed for learning real-time graphics from Deferred PBR to ray tracing.

## 🌟 Features

- **Modern Architecture**: ECS (hecs) + RenderGraph-driven pipeline
- **Cross-Platform**: wgpu automatically targets Vulkan/Metal/DX12
- **Deferred Shading**: G-Buffer-based PBR rendering
- **Extensible**: Add new RenderPasses without touching existing code
- **AI-Friendly**: Modular design, each module fits in a single AI context window

## 🚀 Quick Start

```bash
# Clone
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# Build
cargo build

# Run example (when available)
cargo run --example 01_triangle
```

## 📁 Project Structure

```
├── Cargo.toml
├── crates/
│   └── aether-engine/          # Main engine crate
│       └── src/
│           ├── lib.rs
│           ├── app.rs            # Application entry
│           ├── ecs/              # ECS (hecs wrapper)
│           ├── scene/            # Scene loading/serialization
│           ├── asset/            # Asset management
│           ├── renderer/         # Rendering core
│           │   ├── graph.rs      # RenderGraph
│           │   ├── context.rs    # wgpu context
│           │   └── passes/       # Render passes
│           ├── physics/          # Physics (reserved)
│           ├── math.rs
│           ├── input.rs
│           └── window.rs
├── assets/
│   ├── scenes/                   # .ron scene files
│   ├── shaders/                  # .wgsl shaders
│   ├── meshes/                   # GLTF models
│   └── textures/                 # Textures
├── examples/                     # Example programs
└── openspec/                     # OpenSpec workflow
```

## 🏗️ Architecture

### ECS + RenderGraph

```
App (winit event loop)
  └── SystemRegistry
        ├── Update: Camera, Animation
        └── Render: RenderGraph
              ├── ShadowPass
              ├── GBufferPass
              ├── LightingPass
              ├── SkyboxPass
              ├── PostProcessPass
              └── UIPass
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
| **Phase 0** | Skeleton (window, triangle, egui) | 🚧 In Progress |
| **Phase 1** | Deferred PBR + Shadows + IBL | 🔲 Planned |
| **Phase 2** | SSR + SSAO + Post-Process | 🔲 Planned |
| **Phase 3** | Terrain + Atmosphere + Water + Clouds | 🔲 Planned |
| **Phase 4** | Ray Tracing (Compute + Hybrid) | 🔲 Planned |

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
