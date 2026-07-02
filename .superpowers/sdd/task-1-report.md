# Task 1 Fix Report — Volumetric Cloud Noise Hash Range

## Fixes Applied

1. **`crates/aether-engine/src/renderer/clouds/worley.rs`**
   - `hash3_jitter`: Reinterpret signed `i32` hash as `u32` before float conversion so the full unsigned range maps to `[0.0, 1.0)`.
   - Removed the `(x + 1.0) * 0.5` remap and the redundant `.clamp(0.0, 1.0)`.
   - Corrected module docstring from "3 nearest feature points" to "2 nearest".

2. **`crates/aether-engine/src/renderer/clouds/perlin_worley.rs`**
   - `value_noise_3d` hash closure: Cast `n` to `u32` before float conversion so the full unsigned range maps to `[0.0, 1.0)`.
   - Removed the now-redundant `.clamp(0.0, 1.0)`.

## Verification

```bash
cargo test --workspace --lib
```

Result: **194 passed; 0 failed; 0 ignored**.

## Commit

- SHA: see `git log --oneline -1` (final amended commit)
- Message: `fix(cloud): review fixes for Worley/Perlin-Worley noise hash range`
