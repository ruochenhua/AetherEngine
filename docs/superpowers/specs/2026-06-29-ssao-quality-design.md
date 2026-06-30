# SSAO Quality Enhancement Design

**Issue:** [#88](https://github.com/ruochenhua/AetherEngine/issues/88)  
**Date:** 2026-06-29  
**Status:** Approved

## Summary

Upgrade SSAO from 16-sample hash-rotated kernel to 32-sample stratified hemisphere
with tiled noise rotation, and expand AOBlur from 3×3 to 5×5 bilateral.

## Architecture

### SSAOPass (`ssao.rs`)

#### 1. 32-sample stratified hemisphere kernel

- Current: 16-sample hardcoded `const KERNEL` in WGSL
- New: CPU-side generate 32 samples in `init()`, upload as uniform buffer (bind group 2)
- Stratification: 4 layers × 8 samples each, z-distribution `[0.1, 0.3, 0.6, 0.9]`
  - More samples near hemisphere bottom for contact shadow improvement
- Each sample: random x/y in unit disk, z = layer_z, then normalize to hemisphere

**Uniform struct** (new):

```wgsl
struct SSAOKernel {
    samples: array<vec4<f32>, 32>, // vec4: xyz = direction, w = padding
};
```

**New bind group 2**: kernel uniform buffer + noise texture + noise sampler.

#### 2. 8×8 tiled rotation noise texture

- Generate 8×8 `Rgba8Unorm` texture (64 random 2D unit-vector vectors)
- Bound to bind group 2, sampler with `address_mode: repeat`
- WGSL: `let rot = textureSample(noise_tex, noise_sampler, uv * screen_size / 8.0).rg * 2.0 - 1.0;`
- Replaces `hash2(uv * 1024.0)` — removes `sin()` instruction, hides spatial pattern

#### 3. GBuffer sampler: Nearest → Linear

- SSAO currently samples GBuffer depth/normal with `Nearest`
- Change to `Linear` — in half-resolution a bilinear filter reduces intra-sample jumps

### AOBlurPass (`ao_blur.rs`)

#### 1. 3×3 → 5×5 gaussian + bilateral

- `KERNEL_SIZE: i32 = 1` → `KERNEL_SIZE: i32 = 2`
- 25 weights, sigma=1.2 gaussian precomputed in WGSL `const`
- Bilateral depth-aware logic unchanged (depth-diff weighted by `depth_sigma`)
- At half-resolution, 5×5 covers equivalent of 10×10 at full-res

#### 2. Expose `depth_sigma` uniform

- Current: hardcoded `0.5` in `apply_frame()`
- New: field in `BlurParams`, default `0.5`, read from uniform buffer
- Future: expose in `FrameConfig` for UI control (out of scope)

### What stays the same

| Concern | Decision |
|---------|----------|
| Half-resolution | Keep `half_width/half_height` |
| AO texture format | `R8Unorm` unchanged |
| Pass signatures | Unchanged (same reads/writes relative to screen dims) |
| `FrameConfig` | No new UI knobs in this PR (`depth_sigma` uses default) |
| Scene format | No changes to .ron scene files |

## Data Flow

```
SSAOPass::init()
  ├── generate 32-sample stratified kernel → uniform buffer
  ├── generate 8×8 noise texture → bind group 2
  ├── create pipeline with 3 bind groups:
  │   group 0: GBuffer depth + normal (Linear sampling)
  │   group 1: Frame uniforms (proj/inv_proj/view + params)
  │   group 2: Kernel buffer + noise texture + noise sampler
  └── apply_frame() uploads frame uniforms each frame

AOBlurPass::init()
  ├── create pipeline with 2 bind groups:
  │   group 0: AOTexture + GPosition (Linear sampling)
  │   group 1: BlurParams buffer (depth_sigma + texel_size)
  └── apply_frame() uploads BlurParams each frame
```

## Testing

### Unit tests
- `signature_declares_reads_and_writes` — existing, no change
- `init_creates_resources` — verify 3 bind groups created, kernel has 32 entries
- New: `kernel_is_normalized` — assert all 32 kernel vectors are unit length
- New: `noise_texture_dimensions` — assert 8×8

### Visual verification
- Scene: `scenes/06_ssao_extreme.ron`
- Capture: `--screenshot tests/output/06_ssao_extreme_mode14.png --freeze-time`
- Reference: `tests/reference/06_ssao_extreme_mode14.png` (updated)
- Threshold: SSIM ≥ 0.95
- Manual check: noise visibly reduced, contact shadows tighter near CubeA/B crevice

### Performance baseline
- GPU timer: SSAO + AOBlur combined < 2× current baseline
- Approximate target: SSAO ~0.15ms, AOBlur ~0.12ms (was ~0.08ms + ~0.05ms)

## Future (next PR)

- Blue noise 64×64 texture upgrade
- `depth_sigma` exposed in `FrameConfig` + UI slider
- AO radius depth-scaling for consistent occlusion across distances

## Risks

- **Performance**: 32-sample kernel with 5×5 blur could exceed 2× baseline on low-end GPUs
  - Mitigation: half-resolution already cuts sample count by 4×; SSAO runs post-GBuffer
- **Banding**: Larger blur kernel may over-smooth contact shadows
  - Mitigation: bilateral depth-sigma prevents cross-edge blur; adjustable `depth_sigma`
- **Pattern**: 8×8 tiled noise may show repetition on large flat surfaces
  - Mitigation: combined with stratified kernel randomness; blue noise upgrade path exists
