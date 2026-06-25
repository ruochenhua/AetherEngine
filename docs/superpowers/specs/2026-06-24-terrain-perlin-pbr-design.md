# Terrain Phase 1: Perlin Noise + GPU PBR Splatting

**Tracking issue:** [#102](https://github.com/ruochenhua/AetherEngine/issues/102)  
**Date:** 2026-06-24  
**Status:** Design ready for review  

## Goal
Implement industry-standard terrain rendering for AetherEngine Phase 1: procedural Perlin-noise height generation and GPU-based PBR material splatting.

## Background
- The current `TerrainSource::Procedural` uses layered sine waves, producing repetitive, unrealistic hills.
- The current terrain shader only uses uniform layer colors; it does not sample textures or perform real splatting.
- Kong Engine provides ready-made PBR terrain textures (grass, rock, sand, rock/snow) and a heightmap under `resource/textures/`.

## Selected Approach
**Option B: CPU Perlin + GPU PBR splatting.**

This approach keeps the existing CPU chunk-LOD architecture, adds a new `TerrainSource::Perlin` variant alongside the existing sine-based `Procedural`, and upgrades the fragment shader to sample albedo/normal/roughness/metallic/AO texture arrays and blend layers by height and slope. It offers the best balance of visual improvement and implementation risk.

## Architecture

### Modules changed / added
| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `noise = "0.9"` to `[workspace.dependencies]`. |
| `crates/aether-engine/Cargo.toml` | Reference `noise` with `{ workspace = true }`. |
| `crates/aether-engine/src/terrain/noise.rs` | New module: Perlin / FBM noise helpers. |
| `crates/aether-engine/src/terrain/geometry.rs` | Add `PerlinHeight` implementation of `HeightFunction`; update `estimate_height_range` to read amplitude. |
| `crates/aether-engine/src/scene/config/terrain.rs` | Add `TerrainSource::Perlin { ... }`; extend `TerrainLayerConfig` with height/slope limits and clarify texture path fields. |
| `crates/aether-engine/src/asset/terrain_material.rs` | Build `TerrainGpuTextures` (texture arrays + splat view) during scene load. |
| `crates/aether-engine/src/scene/loader/spawn.rs` | Validate layer count, build GPU texture arrays, and store `TerrainGpuTextures` on the `Terrain` component. |
| `crates/aether-engine/src/renderer/extract.rs` | Pass `TerrainGpuTextures` handles into `OptionalPassData`. |
| `crates/aether-engine/src/renderer/passes/terrain/types.rs` | New module: uniform structs and GPU texture handle types. |
| `crates/aether-engine/src/renderer/passes/terrain/shaders.rs` | Rewrite WGSL fragment shader for texture-array splatting, derivative-based TBN normal blending, and PBR output. |
| `crates/aether-engine/src/renderer/passes/terrain/mod.rs` & `update.rs` | Update bind group layout to include splat map, layer arrays, and samplers. |
| `assets/textures/terrain/` | Copy Kong Engine textures. |
| `scenes/16_terrain_perlin.ron` | New validation scene. |

## Dependency Compatibility Matrix

| Crate | Target Version | Constraint Source | Compatibility Risk | Verification |
|-------|---------------|-------------------|-------------------|--------------|
| `noise` | `0.9` | New terrain height generation | Low — pure Rust, no GPU deps | `cargo tree -d` + `cargo check --all-targets` |

**Verification commands (to run before any implementation code):**
```bash
cargo tree -d                              # ensure no duplicate `noise` major version
cargo check --workspace --all-targets      # baseline compiles
cargo clippy --workspace --all-targets -- -D warnings  # baseline clean
```

If `noise 0.9` conflicts with an existing workspace dependency, pin the latest compatible version and document the reason here.

## Impact Analysis

### Files affected
| File Path | Reference Count | Change Type | Migration Strategy |
|-----------|----------------|-------------|-------------------|
| `crates/aether-engine/src/terrain/geometry.rs` | 1 impl + 2 call sites | Add `PerlinHeight`, update `estimate_height_range` | Direct |
| `crates/aether-engine/src/scene/config/terrain.rs` | 1 struct + loaders | Extend `TerrainSource` and `TerrainLayerConfig` | Direct |
| `crates/aether-engine/src/renderer/passes/terrain/shaders.rs` | 1 shader | Rewrite fragment stage | Direct |
| `crates/aether-engine/src/renderer/passes/terrain/mod.rs` | 1 pass impl | Update bind group layout | Direct |
| `crates/aether-engine/src/renderer/passes/terrain/update.rs` | 1 impl block | Rebuild on config change | Direct |
| `crates/aether-engine/src/renderer/passes/terrain/types.rs` | New | Add uniform & texture handle types | New file |
| `crates/aether-engine/src/renderer/extract.rs` | 1 extraction fn | Pass `TerrainGpuTextures` handle | Direct |
| `crates/aether-engine/src/scene/loader/spawn.rs` | 1 spawn fn | Validate config & build GPU arrays | Direct |
| `crates/aether-engine/src/asset/terrain_material.rs` | 1 material type | Add `TerrainGpuTextures` builder | Direct |
| `Cargo.toml` (workspace + crate) | 2 manifests | Add `noise` dependency | Direct |

**Total directly affected files: 10.** All changes are contained within the `terrain` subsystem; no public trait signatures outside terrain change.

### Public API changes
- `TerrainSource` gains a new enum variant (`Perlin`). Existing variants remain unchanged.
- `TerrainLayerConfig` gains new optional fields; old fields remain.
- No changes to `Pass` trait, `PipelineBuilder`, `Scheduler`, or `SceneDescription` top-level shape.

## Build-Time Safety

- Layer count ≤ 4 is enforced by a dedicated constructor / scene-load validation, not by runtime panics.
- `TerrainGpuTextures` is constructed once during scene load; missing optional textures are replaced by typed placeholder views so the bind group is always valid.
- `PerlinHeight` parameters use `NonZeroU32` for `octaves` or custom validation at construction time to make invalid noise configs unrepresentable.
- `TerrainSource` amplitude drives `estimate_height_range`, eliminating the stale hard-coded `(-128.0, 128.0)` culling bounds.

## Interface Signatures

```rust
// Deterministic, Send + Sync for use across ECS / extract threads
pub struct PerlinHeight {
    seed: u32,
    frequency: f32,
    amplitude: f32,
    octaves: std::num::NonZeroU32,
    persistence: f32,
    lacunarity: f32,
    exponent: f32,
}

impl PerlinHeight {
    pub fn new(seed: u32, frequency: f32, amplitude: f32, octaves: std::num::NonZeroU32) -> Self;
    pub fn with_persistence(self, p: f32) -> Self;
    pub fn with_lacunarity(self, l: f32) -> Self;
    pub fn with_exponent(self, e: f32) -> Self;
}

impl HeightFunction for PerlinHeight {
    fn sample(&self, x: f32, z: f32) -> f32;
}

// GPU-resident texture set; owned by ECS Terrain component
pub struct TerrainGpuTextures {
    pub albedo_array: Arc<wgpu::Texture>,
    pub normal_array: Arc<wgpu::Texture>,
    pub roughness_metallic_array: Arc<wgpu::Texture>,
    pub ao_array: Arc<wgpu::Texture>,
    pub splat_map: Arc<wgpu::Texture>,
}

impl TerrainGpuTextures {
    // Returns a fully built texture set or a structured error.
    pub fn from_material(
        material: &TerrainMaterial,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset_manager: &AssetManager,
    ) -> Result<Self, TerrainTextureError>;
}
```

All texture handles are `Send + Sync + 'static` via `Arc<wgpu::Texture>`; `wgpu::Texture` already satisfies these bounds.

## Noise Generation

- Add `noise = "0.9"` to the workspace root `Cargo.toml` under `[workspace.dependencies]`, then reference it from `crates/aether-engine/Cargo.toml` as `{ workspace = true }`.
- Introduce a `PerlinHeight` struct implementing the existing `HeightFunction` trait (see [Interface Signatures](#interface-signatures) for the exact API).
- The RON config stores `octaves` as `u32` for serde ergonomics; `PerlinHeight::new` accepts `std::num::NonZeroU32` so invalid zero-octave configs are unrepresentable at the type level after validation.
- Internally build `noise::Fbm<noise::Perlin>` and configure it via `set_octaves`, `set_frequency`, `set_persistence`, and `set_lacunarity`.
- `sample(x, z)` samples the FBM, applies the optional `exponent` for terracing, clamps/scales to `[-amplitude, amplitude]`. The `noise` crate returns `f64`; cast to `f32` before scaling.
- Use the seed to deterministically offset the sampled `(x, z)` domain.
- Normals continue to be derived from face-averaged mesh recomputation, which is sufficient for CPU-generated heightfields.
- Update `TerrainPass::estimate_height_range` to derive bounds from the active `TerrainSource` amplitude instead of the current hard-coded `(-128.0, 128.0)`.

## Material Layering

- Support up to 4 layers; validate layer count during scene load.
- Extend the existing `TerrainLayerConfig` fields:
  - Add `min_height`, `max_height`, and `blend_range` for height-based masking.
  - Add `slope_limit` and `blend_range` for slope-based masking.
  - Keep existing albedo/normal/roughness-metallic texture paths; add an optional `ao_texture` path. If possible, keep the existing packed `roughness_metallic_texture` convention and treat metallic as derived from the green channel and roughness from the red channel to minimize asset changes. If a layer provides separate roughness and metallic textures, resize/pad them into a single packed texture at load time.
- Blend weights are computed per pixel in the fragment shader:
  1. Compute a height weight and a slope weight for each layer.
  2. Apply smoothstep transitions using `blend_range`.
  3. Multiply height and slope weights per layer.
  4. Normalize weights and blend albedo, normal, roughness-metallic, and AO.
- If a splat map is provided, its RGBA channels override the procedural weights.
- Normal blending uses UDN-style blending in tangent space. Because the CPU mesh currently has no real tangents, compute the TBN from screen-space derivatives (`dpdx`, `dpdy` of position and UV) in the fragment shader. This avoids changing `geometry.rs` and is standard for triplanar/terrain shading.

## Shader Design

- **Vertex stage:** unchanged except for passing world height, world normal, and layer UV.
- **Fragment stage:**
  - Compute layer blend weights from world height and surface slope.
  - Sample each layer's albedo, normal, packed roughness-metallic, and AO from `texture_2d_array<f32>`.
  - Blend normals in tangent space using UDN blending, then build a TBN from position/UV derivatives and transform to world space.
  - Blend roughness-metallic and AO using the same weights.
  - Output the deferred GBuffer formats already consumed by `LightingPass` and other deferred passes:
    - `GPosition`: `Rgba16Float`
    - `GNormal`: `Rgba16Float`
    - `GAlbedo`: `Rgba8Unorm`
    - `GMaterial`: `Rg8Unorm`
    - `GDepth`: `Depth32Float`
- **Bind group layout changes:**
  - Existing uniform buffer (remains).
  - New `texture_2d<f32>` for the optional splat map.
  - New `texture_2d_array<f32>` for albedo array.
  - New `texture_2d_array<f32>` for normal array.
  - New `texture_2d_array<f32>` for roughness-metallic array.
  - New `texture_2d_array<f32>` for AO array.
  - New sampler(s) for the above.
  - Missing optional textures use a 1×1 placeholder view bound to the same array slot to keep the bind group valid.

## Data Flow

1. `SceneLoader` parses the new `TerrainSource::Perlin` variant and extended `TerrainLayerConfig` entries, validating layer count ≤ 4 and Perlin parameters.
2. `SceneLoader::spawn_terrain` asynchronously loads all CPU textures (splat map and layer textures).
3. Once textures are loaded, a renderer-side helper builds `TerrainGpuTextures`:
   - Resize/pad all layer textures to a common dimension (e.g., 512×512 or 1024×1024) using bilinear filtering.
   - Create four `wgpu::Texture` arrays with `depth_or_array_layers = max(1, layer_count)`.
   - Upload each layer into the corresponding array slice; use a 1×1 neutral placeholder for missing optional textures.
   - Create a `TextureView` for the optional splat map, or a 1×1 white placeholder if absent.
   - Store resulting `Arc<GpuTexture>` handles or `wgpu::TextureView`s in a new `TerrainGpuTextures` struct.
4. `TerrainGpuTextures` is stored on the ECS `Terrain` component (or a dedicated resource) so it survives extraction without needing `AssetManager` in `RenderFrame`.
5. `extract.rs` copies the `TerrainGpuTextures` reference/handle into `OptionalPassData` every frame.
6. `TerrainPass::init` creates a bind group layout matching the new shader resources.
7. `TerrainPass::apply_frame` rebuilds chunk LOD meshes when the terrain config changes.
8. `TerrainPass::execute` binds the textures and draws visible chunks.

## Asset Integration

Copy the following Kong Engine texture sets into `assets/textures/terrain/`:
- `iceland_heightmap.png` (optional heightmap fallback)
- `grass/stylized-grass1_*`
- `rock_shoreline/rocky-shoreline1-*`
- `rock_snow/rock-snow-ice1-2k_*`
- `sand/wavy-sand_*`

Textures are loaded at runtime by path; no manifest changes are required.

## Error Handling

- Missing required albedo textures for a layer: fail at scene load with a clear path.
- Missing optional normal/roughness-metallic/AO textures: load a 1×1 neutral placeholder (normal = (0.5, 0.5, 1.0), roughness-metallic = (0.5, 0.0), AO = 1.0) so the shader bind group remains valid.
- Layer counts greater than 4: fail at scene load with a clear error.
- Invalid Perlin parameters (`octaves == 0`, `frequency <= 0`, `amplitude < 0`, `persistence <= 0`, `lacunarity <= 0`): validate in `spawn_terrain` or a custom deserializer and fail fast.
- Texture size mismatch: resize/pad all layer textures to a common dimension at load time; log a single warning per layer set.
- GPU texture array creation failure: propagate as `PipelineBuildError` or log and skip terrain pass.
- Editor overlay warnings are optional for Phase 1; console logging via `tracing::warn!` is sufficient.

## AI-Agent Development Notes

This design follows AetherEngine's AI-first conventions:

- **Module size:** Every new/modified Rust module stays below 500 LOC. If `terrain_material.rs` or `shaders.rs` grows past the limit, split into sub-modules (`terrain_material/`, `shaders/`).
- **One file = one context:** The WGSL shader remains inline in `shaders.rs` so a future agent can read the complete pass in one context window.
- **TDD + Visual Verify:** Implement in the order RED → GREEN → **VISUAL VERIFY** → REFACTOR. Any change under `renderer/passes/terrain/` is `MUST_VERIFY` per `docs/agents/visual-test-workflow.md`.
- **Public-interface tests only:** Unit tests for `PerlinHeight` go through `HeightFunction::sample`; tests for material blending go through the public `TerrainLayerConfig`/`TerrainGpuTextures` builders. Do not test private helpers in isolation.
- **Build-time safety:** Invalid configs are rejected at deserialization or construction time, not at runtime inside the pass.
- **Issue/worktree isolation:** This change is tracked by issue #102 and should be developed in its own git worktree if the `using-git-worktrees` skill is active.

## Optional Internal Phasing

To reduce integration risk, the implementation can be split into two internal milestones:

1. **Milestone B1 — Perlin height + procedural uniform blending:**
   - Add `TerrainSource::Perlin`, `PerlinHeight`, and height/slope-based layer weights.
   - Keep the existing uniform-color shader path; use the new weights to blend the existing `albedo`/`roughness`/`metallic` colors.
   - This validates the noise, config, and blending logic without touching bind groups or texture arrays.

2. **Milestone B2 — GPU texture-array splatting:**
   - Build `TerrainGpuTextures`, update bind group layout, and rewrite the fragment shader to sample texture arrays.
   - Copy Kong Engine textures and wire them into `16_terrain_perlin.ron`.

Both milestones together fulfill the acceptance criteria; B1 can be merged independently if desired.

## Testing

### Unit tests (RED → GREEN)
- Deterministic `PerlinHeight::sample` output for identical seed/coordinates.
- FBM output stays within `[-amplitude, amplitude]`.
- Height/slope weight blending boundary cases.
- `TerrainGpuTextures::from_material` returns `Err` on invalid layer count / missing required albedo.

### Integration / compile checks
- `16_terrain_perlin.ron` loads and builds a render pipeline without errors.
- `cargo test --workspace --lib` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo fmt --check` passes.

### Visual verification (VISUAL VERIFY) — mandatory
Because this change touches `renderer/passes/terrain/`, the following visual workflow must run before the issue is closed:

1. Run the smart gate:
   ```bash
   python3 scripts/should-verify-visual.py --since HEAD~1
   ```
2. If `MUST_VERIFY` (expected), launch the launcher and capture `16_terrain_perlin.ron`:
   ```bash
   cargo run --bin aether-launcher -- --scene scenes/16_terrain_perlin.ron --screenshot tests/output/16_terrain_perlin.png
   ```
3. Inspect the output for:
   - Visible Perlin terrain variation (not sine-like hills).
   - At least 3 distinct material layers (e.g., sand → grass → rock/snow).
   - Normal-map surface detail and correct lighting response.
   - No LOD cracks or seams.
4. If this is the first validation, promote the screenshot to reference:
   ```bash
   cp tests/output/16_terrain_perlin.png tests/reference/16_terrain_perlin.png
   ```
5. Write a report to `tests/reports/YYYY-MM-DD-16_terrain_perlin.md` using the template in `docs/agents/visual-test-workflow.md`.

## Acceptance Criteria
- [ ] `TerrainSource::Perlin` generates chunked LOD terrain with FBM Perlin noise.
- [ ] Terrain shader samples albedo/normal/roughness/metallic/AO texture arrays.
- [ ] Height- and slope-based material blending shows at least 3 visible layers.
- [ ] New scene `16_terrain_perlin.ron` renders correctly in the launcher.
- [ ] All existing tests, clippy, and formatting checks pass.
- [ ] Visual verification report exists in `tests/reports/` and is marked PASS.
- [ ] Issue #102 closed only after `needs-visual-verify` label is resolved.

## Task Breakdown Preview

> Follows `openspec/workflow-guides/task-template.md`. Total tasks ≈ 10; if it grows beyond 15, split B1/B2 into separate specs.

### Task 1: Dependency & baseline
- Add `noise = "0.9"` to workspace `[workspace.dependencies]` and crate reference.
- Run `cargo tree -d`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **AC:** baseline clean.

### Task 2: Perlin height generation
- Create `crates/aether-engine/src/terrain/noise.rs` and `PerlinHeight`.
- Add unit tests for determinism, amplitude bounds, and seed variation.
- **AC:** `cargo test -p aether-engine terrain::noise` passes.

### Task 3: Terrain config extension
- Extend `TerrainSource` and `TerrainLayerConfig` with validation.
- Update serialization tests.
- **AC:** `cargo test -p aether-engine scene::config` passes.

### Task 4: GPU texture array builder
- Add `TerrainGpuTextures` and `TerrainTextureError`.
- Implement resize/pad and placeholder logic.
- **AC:** builder unit tests pass; invalid input returns structured errors.

### Task 5: Scene loader integration
- Wire `TerrainGpuTextures::from_material` into `spawn_terrain`.
- Store result on ECS `Terrain` component; update `extract.rs`.
- **AC:** `16_terrain_perlin.ron` loads without panic.

### Task 6: Terrain shader rewrite
- Update WGSL for texture-array splatting and derivative-based TBN.
- Update bind group layout in `mod.rs`.
- **AC:** shader compiles (`cargo check`); no validation errors at runtime.

### Task 7: Compile verification
- `cargo check --workspace --all-targets` 0 errors, 0 new warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo fmt --check` passes.

### Task 8: Runtime verification
- Launch `16_terrain_perlin.ron`; confirm no panic / wgpu validation error.
- Capture screenshot and inspect material layers.

### Task 9: Visual verification & report
- Run `scripts/should-verify-visual.py`.
- Generate/update reference image and write `tests/reports/YYYY-MM-DD-16_terrain_perlin.md`.
- Add `needs-visual-verify` label to #102, then resolve it.

## Depends On
- None. This is the first issue in the terrain / atmosphere / clouds improvement track.

## Blocks
- [#103] Atmosphere Phase 2 improvements (optional, can proceed in parallel after B1).
- [#104] Volumetric Cloud Phase 2 improvements (optional, can proceed in parallel after B1).

## Out of Scope
- GPU tessellation or compute-shader displacement (WebGPU has no tessellation stage).
- Runtime terrain editing or streaming infinite worlds.
- Atmospheric scattering improvements (Phase 3) and volumetric cloud upgrades (Phase 2).

## References
- Kong Engine textures: https://github.com/ruochenhua/KongEngine/tree/master/resource/textures
- Kong Engine terrain shaders: `resource/shader/terrain/terrain_tess.*`
