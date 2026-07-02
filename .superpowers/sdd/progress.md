# Subagent-Driven Development Progress Ledger

Issue: volumetric-cloud-redesign
Worktree: .worktrees/feature/volumetric-cloud-redesign
Branch: feature/volumetric-cloud-redesign
Base: aaac6b1

## Tasks
- [x] Task 1: Worley + Perlin-Worley noise generators (7fcd337, review approved with reservations)
- [x] Task 2: Curl noise generator (dfd6bee, review approved)
- [x] Task 3: Weather map + CloudQuality config (2a55c48, review approved)
- [x] Task 4: Multi-noise upload + bind group in VolumetricCloudPass (f2e96f8, review approved)
- [x] Task 5: New WGSL shader with HG phase + self-shadowing (a531423, review approved)
- [x] Task 6: CloudQuality presets in execute + scene update (2e5db74, review approved)
- [x] Task 7: Integration — variable noise texture sizes per quality (TBD, pending review)

## Notes
- Task 1 implementation intentionally deviates from brief Step 6's reference formula: the brief's all-Perlin base did not actually use Worley despite the function name/docstring. Review fix made it a true Worley + Perlin blend, which matches the project goal of Nubis-style multi-noise clouds.

## Task 2 Minor findings (deferred to final review)
- curl.rs:51 comment says "Map [-1, 1] to [-128, 127]" but code maps to [-127, 127].
- curl.rs:134-135 `x <= 127` / `y <= 127` trigger `unused_comparisons` warnings.

## Task 4 Minor findings (deferred to final review)
- mod.rs:34 `#[allow(dead_code)]` on `noise_bind_group` may be unnecessary (used in execute.rs).
- mod.rs:141-143 quality_params/cloud_color default every frame; configurable in Task 6.
- mod.rs:1-7 module doc comment still refers to single noise texture.

## Task 5 notes
- Shader source lives in `shader.rs` instead of `pipeline.rs` because `pipeline.rs` was split in Task 4 review fix. Functionally equivalent.
- Report file contains minor commit-hash typo; does not affect code.
