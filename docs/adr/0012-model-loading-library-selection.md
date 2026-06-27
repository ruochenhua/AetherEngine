# ADR-0012: Model Loading Library Selection — gltf + tobj, Not assimp

## Status

Accepted

## Context

AetherEngine currently has stubbed external model loading in `asset/mesh.rs`:

```rust
fn load_obj(_path: &Path) -> anyhow::Result<CpuMesh> {
    anyhow::bail!("OBJ loading not yet implemented")
}
fn load_gltf(_path: &Path) -> anyhow::Result<CpuMesh> {
    anyhow::bail!("GLTF mesh loading not yet implemented")
}
```

The previous engine iteration used **assimp** (via its C++ API) to import a wide
variety of model formats. In the current architecture — Rust + wgpu, ECS-based
scene management, `CpuMesh`/`GpuMesh` upload, and deferred PBR — we need to
choose a replacement that is idiomatic, maintainable, and compatible with the
existing asset pipeline.

The deferred PBR vertex layout expects:

- `position: [f32; 3]`
- `normal: [f32; 3]`
- `uv: [f32; 2]`
- `tangent: [f32; 4]`

Materials need albedo/roughness/metallic values plus optional albedo textures
that can be resolved through the existing `AssetManager`/`GpuTextureCache`.

## Decision

Use the **pure-Rust `gltf` crate for `.gltf` / `.glb`** and the **pure-Rust
`tobj` crate for `.obj`**. Do **not** introduce `russimp` or the native assimp
library as a core dependency.

### Key design points

1. **Primary format: glTF 2.0 (`gltf` crate)**
   - glTF is the modern runtime asset standard and maps cleanly to PBR
     (metallic-roughness workflow), texture samplers, node hierarchies,
     animations, and skins.
   - The `gltf` crate (v1.4) is mature, actively maintained, and supports
     extensions such as `KHR_lights_punctual`, `KHR_texture_transform`, and
     `KHR_materials_*`.
   - The crate already handles buffer/image import via `gltf::import` and
     provides accessor readers for positions, normals, UVs, tangents, and
     indices.
   - AetherEngine already declares `gltf` in `Cargo.toml` (workspace dependency).

2. **Legacy / simple format: Wavefront OBJ (`tobj` crate)**
   - `tobj` is a small, fast OBJ/MTL loader with no C dependencies.
   - It returns flat `Vec<f32>` buffers and indices, which map directly to
     `CpuMesh`.
   - The workspace already declares `tobj = { version = "4.0",
     default-features = false }`.
   - OBJ will be supported as a basic import path; material textures declared in
     the companion `.mtl` file will be loaded through `AssetManager` when
     referenced by a scene object.

3. **Rejected: assimp / `russimp`**
   - assimp is a large C++ library. Bringing it in via `russimp` or
     `russimp-sys` adds CMake, libclang, and platform-specific build
     requirements.
   - assimp's broad format support is valuable, but most production assets for a
     wgpu engine are exported as glTF or converted to glTF offline.
   - For a Rust-first project, relying on a heavy non-Rust dependency for the
     primary model loader conflicts with the goal of fast, reproducible builds
     and cross-platform (including wasm) portability.

4. **Integration approach**
   - `CpuMesh::load(path)` will dispatch on extension:
     - `.obj` → `tobj` loader
     - `.gltf` / `.glb` → `gltf` loader
   - The glTF loader will read the default scene, traverse nodes, apply node
     transforms, and merge primitive geometry into a single `CpuMesh` per file
     for Phase 1. Materials and transforms will be extracted so the
     `SceneLoader` can spawn the appropriate `MeshRenderer` entities.
   - Tangents will be generated with `glam` cross products when the source file
     does not provide them.
   - Texture references in glTF/OBJ materials will be returned as relative paths
     and resolved through the existing `AssetManager::load::<CpuTexture>` path,
     then sampled via `GpuTextureCache`.

## Considered Options

- **Keep assimp via `russimp`**:
  rejected — heavy C++ build dependency, platform fragility, poor fit for a
  Rust/wgpu project aiming at reproducible builds and future wasm support.

- **Use only `gltf` and drop OBJ support**:
  rejected — OBJ is still common for quick prototypes and legacy assets.
  `tobj` is cheap enough to keep as a secondary loader.

- **Use `morph3d` or another multi-format loader**:
  rejected — less mature, smaller community, and does not provide the glTF
  feature coverage or maintenance track record of the `gltf` crate.

- **Write a custom OBJ loader**:
  rejected — `tobj` already covers the format well; reimplementing it adds
  maintenance burden without benefit.

## Consequences

- **Pros**:
  - Pure-Rust dependency stack keeps builds fast and portable.
  - glTF is the natural format for PBR/wgpu workflows and supports the data we
    need (meshes, materials, textures, animations).
  - Both `gltf` and `tobj` are already declared in the workspace, so
    integration requires no new top-level dependencies.
  - `CpuMesh` and `GpuMesh` remain the single source of truth for geometry;
    loaders are thin adapters.

- **Cons**:
  - assimp's broad format support (FBX, COLLADA, 3DS, etc.) is no longer
    available out-of-the-box. Assets in those formats must be converted to
    glTF/OBJ offline (e.g., with Blender or glTF-Pipeline).
  - glTF scene import can be complex (skins, animations, multiple primitives
    per mesh). The initial implementation will focus on static meshes and
    PBR materials; advanced features will be added incrementally.
  - `tobj` supports only a subset of OBJ/MTL; exotic MTL features may not map
    cleanly to our PBR material model.
