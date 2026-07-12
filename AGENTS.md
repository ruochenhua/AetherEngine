# Aether Engine Agent Rules

## Change-What-Test-What Rule

For every code change, the agent must run the tests and runtime verification that correspond to the changed code. Do not rely on "it compiled" or "unit tests passed" alone when the change affects the render graph, pipeline registration, or launcher startup.

## Concrete Verification Checklists

### Render graph / pipeline builder / scheduler / resource table changes
- Run: `cargo test -p aether-engine --lib`
- Run: `cargo build -p aether-launcher --release`
- Run launcher and verify it reaches the first frame without panic:
  ```
  E:/Users/ruochenhua/.cargo/aether-target/release/aether-launcher.exe --scene scenes/13_clouds.ron
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

## Prohibited Shortcuts

- **Never `git checkout -- <file>` to "revert diagnostics"** without first reviewing the diff and ensuring production code (imports, pass registration, struct fields, feature wiring) is preserved.
- Do not remove `tracing`/`info!` logs by checking out entire files. Remove only the log lines you added.

## Before Marking a Task Done

1. The changed crate(s) compile without errors.
2. All relevant unit/integration tests pass.
3. If the launcher is affected, a release build exists and launches successfully with a relevant scene.
4. Any regression from the change has a test added or a scene verification step documented.
