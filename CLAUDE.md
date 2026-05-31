# AetherEngine

Rust + wgpu deferred renderer. Inherited from KongEngine.

## Project Philosophy

**AI-first codebase.** Every architectural decision optimizes for AI agents as primary developers, humans as reviewers. See `README.md` § AI-First Design for details.

## Agent skills

### Issue tracker

GitHub Issues (https://github.com/ruochenhua/AetherEngine). Uses `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default canonical labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: `CONTEXT.md` at repo root + `docs/adr/` for architectural decisions. See `docs/agents/domain.md`.

## Development Conventions

### TDD (Test-Driven Development)

- Write the failing test first (RED), then minimal code to pass (GREEN), then refactor.
- Tests verify behavior through **public interfaces only**. Never test private functions.
- Build-time errors preferred over runtime errors — use types to make invalid states unrepresentable.

See `docs/agents/tdd.md` for full workflow.

### Pass Architecture

- Every render pass implements the `Pass` trait: `signature()` → `init()` → `resolve()` → `execute()`.
- Adding a pass: copy `renderer/passes/template.rs`, fill in signature + shader, register one line in `build_pipeline()`.
- Resource wiring is type-safe: `ResHandle<GPosition>` ≠ `ResHandle<GNormal>`. Compiler catches mistakes.

### Module Size

- Each module < 500 LOC. AI reads the whole module in one context window.
- If a file exceeds 500 LOC, extract a sub-module.

### Shaders

- WGSL shaders are inline in Rust pass files. One file = complete context.
- Use `r#"..."#` raw strings + `Cow::Borrowed` for shader source.

## Key Files

| File | Role |
|------|------|
| `CONTEXT.md` | Domain glossary (terms + _Avoid_) |
| `docs/adr/` | Architectural Decision Records |
| `README.md` | Project overview + AI-first philosophy |
| `crates/aether-engine/src/renderer/pass.rs` | Pass trait definition |
| `crates/aether-engine/src/renderer/scheduler.rs` | Scheduler + PipelineBuilder |
| `crates/aether-launcher/src/main.rs` | Thin orchestration layer |

## Known Pitfalls

- **wgpu MRT**: Single `fs_main` returning `FragmentOutput` struct (not multiple entry points)
- **Windows include_str!**: May produce ghost files; use `r#"..."#` + `Cow::Borrowed`
- **Normal encoding**: GBuffer `*0.5+0.5`, Lighting `*2.0-1.0`
- **Surface `'static`**: `Arc<Window>` needed for lifetime
- **Fullscreen quad UV flip**: `uv = vec2(x*0.5+0.5, 0.5 - y*0.5)` — wgpu NDC Y=1 is top
- **FlyCam right() cross order**: `forward × world_up` (correct) vs `world_up × forward` (wrong)
