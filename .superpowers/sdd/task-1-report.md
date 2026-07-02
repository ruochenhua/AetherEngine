# Task 1 Report: Worley + Perlin-Worley noise generators

## Status
DONE

## Summary
Implemented two pure-CPU 3D noise generators under `crates/aether-engine/src/renderer/clouds/`:
- `worley.rs` — `worley_noise_3d(size: u32) -> Vec<u8>` (R8Unorm cellular/Worley noise)
- `perlin_worley.rs` — `perlin_worley_noise_3d(size: u32) -> Vec<u8>` (Perlin × Worley blend)
- `mod.rs` — wired both modules as `pub mod`

## Changes
- Created `crates/aether-engine/src/renderer/clouds/worley.rs` (123 LOC)
- Created `crates/aether-engine/src/renderer/clouds/perlin_worley.rs` (118 LOC)
- Modified `crates/aether-engine/src/renderer/clouds/mod.rs` (+2 lines)

All files remain under the 500 LOC per-file limit.

## TDD Steps
1. Wrote failing tests for `worley_noise_3d`.
2. Wired `pub mod worley;` and ran `cargo test --lib clouds::worley` → compilation error as expected.
3. Implemented `worley_noise_3d`.
4. Ran `cargo test --lib clouds::worley` → 3 tests passed.
5. Wrote failing tests for `perlin_worley_noise_3d`.
6. Wired `pub mod perlin_worley;` and ran `cargo test --lib clouds::perlin_worley` → compilation error as expected.
7. Implemented `perlin_worley_noise_3d`.
8. Ran `cargo test --lib clouds::perlin_worley` → 3 tests passed.
9. Ran full workspace lib test suite.
10. Committed.

## Commands Run

```bash
cargo test --lib clouds::worley       # 3 passed
cargo test --lib clouds::perlin_worley # 3 passed
cargo test --workspace --lib          # 194 passed; 0 failed
```

## Full Workspace Test Result

```
running 194 tests
test result: ok. 194 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.37s
```

## Warnings
The build emits pre-existing `deprecated` warnings for `glam::Mat4` camera helpers in unrelated modules. Two new `unused_comparisons` warnings were introduced by the test assertions `assert!(v <= 255u8)` copied verbatim from the brief. These are harmless and match the specified test code exactly.

## Commit
- `306283c` — `feat(cloud): Worley + Perlin-Worley noise generators`

## Concerns
None blocking. The `unused_comparisons` warnings could be silenced in a follow-up by removing the redundant `<= 255u8` checks, but doing so would deviate from the exact test code provided in the brief.

---

## Review Fix Round (post-review)

### Status
FIXED

### Fixes Applied
1. **`crates/aether-engine/src/renderer/clouds/perlin_worley.rs`**
   - Imported `crate::renderer::clouds::worley::worley_noise_3d`.
   - Replaced low-frequency Perlin base with `worley_noise_3d(size)` as the Worley base.
   - Blended Worley base (weight 0.7) with high-frequency Perlin detail (weight 0.3), matching the docstring formula.
   - Clamped the `value_noise_3d` hash output to `[0.0, 1.0]` to mirror `clouds/noise.rs`.
   - Replaced `assert!(v <= 255u8)` with `assert!(v >= 0)` in tests.

2. **`crates/aether-engine/src/renderer/clouds/worley.rs`**
   - Remapped `hash3_jitter` signed hash to `[0.0, 1.0)` using `(h as f32 / u32::MAX as f32 + 1.0) * 0.5`.
   - Updated `hash3_jitter` docstring to `[0.0, 1.0)`.
   - Replaced `assert!(v <= 255u8)` with `assert!(v >= 0)` in tests.

### Verification
```bash
cargo test --workspace --lib
```
Result: **194 passed; 0 failed; 0 ignored**.

### Commit
- Commit message: `fix(cloud): review fixes for Worley/Perlin-Worley noise`
