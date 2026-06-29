# Task 1: Dependency & Baseline

## Files
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/aether-engine/Cargo.toml`

## Goal
Add the `noise` crate as a workspace dependency and verify the baseline still compiles and passes clippy.

## Exact Steps
1. In `Cargo.toml`, inside `[workspace.dependencies]`, add:
   ```toml
   noise = "0.9"
   ```
2. In `crates/aether-engine/Cargo.toml`, inside `[dependencies]`, add after `thiserror = { workspace = true }`:
   ```toml
   noise = { workspace = true }
   ```
3. Run and verify output:
   ```bash
   cargo tree -d | grep noise
   cargo check --workspace --all-targets
   cargo clippy --workspace --all-targets -- -D warnings
   ```
   Expected: only one `noise` entry in `cargo tree -d`; `check` and `clippy` pass with no new errors/warnings.

## Commit
```bash
git add Cargo.toml crates/aether-engine/Cargo.toml
git commit -m "deps(#102): add noise 0.9 for Perlin terrain generation"
```

## Report
Write a brief report to `.superpowers/sdd/task-1-report.md` with status, commands run, and output summary.
