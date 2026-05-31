# ADR-0002: RON Scenes replace Example trait

## Status

Accepted

## Context

ADR-0001 established the `Example` trait as the scene-switching unit in the Launcher. Each scene was a Rust struct implementing `init` / `update` / `prepare` / `render` / `ui` / `cleanup`. This had three problems:

1. **Duplication**: Every example recreated the same infrastructure (FlyCamera, DebugGrid/Gizmo, GBufferPass, LightingPass, GBuffer). Different examples sometimes used different camera types (OrbitCamera vs FlyCamera).
2. **Friction**: Adding a new scene required writing Rust code — a struct, trait impl, and Launcher registration entry.
3. **Incoherent UX**: Different examples had different control schemes, debug panels, and key bindings. No unified experience across scenes.

The goal was to unify all infrastructure in the Launcher layer and make scenes purely declarative.

## Decision

Abolish the `Example` trait. All scenes become RON files in a `scenes/` directory, discovered by the Launcher via directory scanning.

The Launcher owns — once, for the entire process lifetime — the full deferred rendering pipeline (GBufferPass → LightingPass → DebugLinePass → egui), the FlyCamera (the only camera type), InputManager, and DebugGrid/Gizmo vertex buffers.

A scene file describes only the data that varies between scenes: camera initial position/yaw/pitch, lights, and a list of objects (mesh reference + transform + PBR material). The Launcher's `SceneLoader` converts this into GPU `Renderable` instances and `LightingUniforms` at scene-switch time.

GPU resources are not shared across scene switches — each switch tears down and rebuilds `Renderable` lists and GpuMesh instances. This keeps the implementation simple.

The `BuiltinMeshRegistry` maps string names (`"cube"`, `"sphere"`, `"quad"`) to `CpuMesh` factory functions. Objects in RON can reference built-in meshes via `#name` prefix or external files via path.

Scene behaviour is purely static in Phase 1 — no scripts, no animation, no physics. The architecture reserves the ECS module for future behaviour extensions.

## Consequences

### Pros
- Zero Rust boilerplate to add a scene; drop a `.ron` file in `scenes/`.
- Unified control scheme across all scenes (FlyCamera, same key bindings).
- Unified debug overlay (grid + gizmo always present, same toggles).
- Launcher is the single owner of all GPU pipeline state — coherent, testable.

### Cons
- Scenes that don't fit the deferred pipeline (e.g. compute-only or raytrace demos) have no escape hatch. Will need to revisit when Phase 4 (Ray Tracing) arrives.
- The `01_triangle` example is deleted — a minor regression in the learning curve.
- Standalone `cargo run --example` support is removed. All scenes launch through the Launcher.

## Considered Options

- **Keep Example trait as escape hatch**: Rejected because Phase 1 has no use for it. Can be reintroduced when a concrete need arises.
- **YAML instead of RON**: Rejected. RON is Rust-native, serde-compatible, and already the existing format. YAML adds a dependency for no benefit at this stage.
- **GpuMesh cache across scene switches**: Rejected for Phase 1 to keep implementation simple.
