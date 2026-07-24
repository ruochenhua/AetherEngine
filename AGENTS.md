# Aether Engine Agent Rules

## Change-What-Test-What Rule

For every code change, the agent must run the tests and runtime verification that correspond to the changed code. Do not rely on "it compiled" or "unit tests passed" alone when the change affects the render graph, pipeline registration, or launcher startup.

## Concrete Verification Checklists

### Render graph / pipeline builder / scheduler / resource table changes
- Run: `cargo test -p aether-engine --lib`
- Run: `cargo build -p aether-launcher --release`
- Run launcher and verify it reaches the first frame without panic:
  ```
  target/release/aether-launcher.exe --scene scenes/13_clouds.ron
  ```

### Launcher pipeline registration changes (`crates/aether-launcher/src/pipeline.rs`, pass add/remove)
- Run: `cargo build -p aether-launcher --release`
- Run launcher with a scene that exercises the affected passes:
  - Clouds / motion vector / temporal: `scenes/13_clouds.ron`
  - Terrain: `scenes/08_terrain.ron` or any terrain scene
  - SSR: `scenes/07_ssr_debug.ron` or equivalent

### Volumetric cloud / atmosphere / lighting changes
- Run: `cargo test -p aether-engine --lib`
- Run launcher release with `scenes/13_clouds.ron` and visually verify the sky/clouds render.

### Scene loader / config changes
- Run: `cargo test -p aether-engine --lib`
- Run launcher release with the affected `.ron` scenes and verify they load.

## Shader Conventions

- Every new shader must be registered in the shader validation manifest (`crates/aether-engine/src/renderer/shader_validation.rs`, `SHADER_MANIFEST`): expose its WGSL source as a `pub(crate) const` so the CPU-only naga validation test covers it.
- Every `create_shader_module` call must use a descriptive label (e.g. "SSR Trace Shader", never "S" or "transient") — the label is what wgpu validation error reports show.

## GPU Resource Conventions

- **Replace-to-free**: any field that holds a GPU resource (`wgpu::Texture`, `Buffer`, `BindGroup`, `Arc<GpuTexture>`, …) must be updated by wholesale replacement — dropping the old value releases its device memory. Never mutate such a resource in place across frames in a way that keeps the old allocation alive.
- **Cross-scene caches must be evictable**: any cache of GPU resources that outlives a single scene (e.g. `GpuTextureCache`) must expose an eviction interface (`clear()`) and that interface must be called on every scene switch (open / new scene). CPU-side registries that dedup by path may be retained so long as their ids stay stable and cannot alias a different cached GPU entry.

## Prohibited Shortcuts

- **Never `git checkout -- <file>` to "revert diagnostics"** without first reviewing the diff and ensuring production code (imports, pass registration, struct fields, feature wiring) is preserved.
- Do not remove `tracing`/`info!` logs by checking out entire files. Remove only the log lines you added.

## Before Marking a Task Done

1. The changed crate(s) compile without errors.
2. All relevant unit/integration tests pass.
3. If the launcher is affected, a release build exists and launches successfully with a relevant scene.
4. Any regression from the change has a test added or a scene verification step documented.
