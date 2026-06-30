# SSAO Quality Enhancement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade SSAO from 16-sample hash-rotated kernel to 32-sample stratified hemisphere with 8×8 tiled noise rotation, and expand AOBlur from 3×3 to 5×5 bilateral.

**Architecture:** Two-pass change. SSAOPass gets a 32-sample uniform kernel buffer + 8×8 noise texture in a new bind group 2, and switches GBuffer sampling from Nearest to Linear. AOBlurPass expands kernel radius + uses existing `depth_sigma` uniform. Pipeline layout and WGSL must change atomically with BGL — no intermediate state.

**Tech Stack:** Rust + wgpu + WGSL

## Global Constraints

- `rand` crate not available; use precomputed `const` arrays for deterministic kernel/noise
- Half-resolution (`half_width/half_height`) unchanged
- AO texture format `R8Unorm` unchanged
- Pass signatures unchanged
- `FrameConfig` unchanged (no new UI knobs in this PR)
- All workspace lib tests must pass
- No new clippy warnings

---

### Task 1: SSAO — 32-sample kernel + 8×8 noise + WGSL rewrite + Linear sampler

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/ssao.rs`

**Interfaces:**
- Consumes: `wgpu::Device`, `wgpu::Queue`, existing `SSAOPass` struct
- Produces: `SSAOKernelUniform` struct, kernel uniform buffer, 8×8 noise texture, bind group 2 (kernel+noise+sampler), 3-group pipeline layout, rewritten WGSL, Linear GBuffer sampler

- [ ] **Step 1: Add kernel uniform struct and precomputed 32-sample kernel**

Add after `SSAOFrameUniforms` (before `pub struct SSAOPass`):

```rust
/// 32-sample stratified hemisphere kernel (4 layers × 8 samples).
/// xyz = normalized direction in tangent space, w = padding.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SSAOKernelUniform {
    samples: [[f32; 4]; 32],
}

/// Precomputed 32-sample stratified hemisphere kernel.
/// Layer z-distribution: [0.1, 0.3, 0.6, 0.9] — 8 samples per layer.
const KERNEL_SAMPLES: [[f32; 3]; 32] = [
    // Layer 0: z ~ 0.1 (near hemisphere bottom, contact shadow emphasis)
    [-0.517, 0.089, 0.103], [0.135, -0.628, 0.108], [0.419, 0.473, 0.099],
    [-0.601, -0.309, 0.112], [0.732, -0.158, 0.109], [-0.261, 0.692, 0.105],
    [-0.843, 0.175, 0.107], [0.088, -0.866, 0.101],
    // Layer 1: z ~ 0.3
    [-0.409, 0.127, 0.301], [0.573, 0.361, 0.299], [-0.221, -0.632, 0.308],
    [0.309, -0.577, 0.304], [-0.688, 0.417, 0.305], [0.812, 0.091, 0.302],
    [-0.455, -0.245, 0.299], [0.057, 0.831, 0.303],
    // Layer 2: z ~ 0.6
    [-0.283, 0.212, 0.601], [0.661, -0.143, 0.599], [-0.504, 0.473, 0.602],
    [0.413, 0.559, 0.598], [-0.118, -0.694, 0.603], [0.249, -0.667, 0.599],
    [-0.727, -0.308, 0.604], [0.587, 0.412, 0.600],
    // Layer 3: z ~ 0.9 (top of hemisphere, distant occlusion)
    [-0.174, 0.114, 0.897], [0.513, 0.209, 0.898], [-0.351, -0.398, 0.895],
    [0.383, -0.377, 0.901], [-0.528, 0.291, 0.896], [0.196, 0.571, 0.903],
    [-0.608, -0.173, 0.902], [0.431, 0.481, 0.899],
];
```

- [ ] **Step 2: Generate kernel buffer + noise texture in `SSAOPass::new()`**

In `SSAOPass::new()`, after the quad vertex buffer creation and before `Self {`, insert kernel buffer creation:

```rust
        // Build 32-sample kernel uniform buffer (with padding to vec4)
        let mut kernel_data: [[f32; 4]; 32] = [[0.0; 4]; 32];
        for (i, s) in KERNEL_SAMPLES.iter().enumerate() {
            kernel_data[i] = [s[0], s[1], s[2], 0.0];
        }
        let kernel_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSAO Kernel"),
            contents: bytemuck::cast_slice(&kernel_data),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // 8×8 tiled rotation noise texture (64 random 2D vectors)
        let noise_data: [[f32; 4]; 64] = [
            [0.426, 0.322, 0.0, 0.0], [-0.803, 0.463, 0.0, 0.0],
            [0.121, -0.791, 0.0, 0.0], [-0.477, -0.638, 0.0, 0.0],
            [0.713, 0.557, 0.0, 0.0], [-0.215, 0.834, 0.0, 0.0],
            [0.895, -0.207, 0.0, 0.0], [-0.941, -0.156, 0.0, 0.0],
            [0.334, -0.668, 0.0, 0.0], [0.572, 0.689, 0.0, 0.0],
            [-0.638, 0.579, 0.0, 0.0], [-0.275, -0.716, 0.0, 0.0],
            [0.801, 0.382, 0.0, 0.0], [-0.485, 0.688, 0.0, 0.0],
            [0.092, 0.842, 0.0, 0.0], [0.742, -0.497, 0.0, 0.0],
            [-0.539, -0.586, 0.0, 0.0], [0.367, -0.677, 0.0, 0.0],
            [0.947, 0.119, 0.0, 0.0], [-0.769, -0.459, 0.0, 0.0],
            [0.239, 0.799, 0.0, 0.0], [-0.098, 0.825, 0.0, 0.0],
            [0.611, -0.569, 0.0, 0.0], [-0.369, -0.687, 0.0, 0.0],
            [0.853, 0.334, 0.0, 0.0], [-0.678, 0.528, 0.0, 0.0],
            [0.503, -0.651, 0.0, 0.0], [-0.884, -0.239, 0.0, 0.0],
            [0.291, 0.751, 0.0, 0.0], [0.658, 0.563, 0.0, 0.0],
            [-0.416, 0.720, 0.0, 0.0], [-0.147, -0.809, 0.0, 0.0],
            [0.787, -0.401, 0.0, 0.0], [-0.553, -0.631, 0.0, 0.0],
            [0.119, 0.851, 0.0, 0.0], [0.449, -0.667, 0.0, 0.0],
            [-0.726, 0.501, 0.0, 0.0], [-0.231, -0.751, 0.0, 0.0],
            [0.965, 0.089, 0.0, 0.0], [-0.527, 0.678, 0.0, 0.0],
            [0.357, 0.768, 0.0, 0.0], [-0.845, -0.309, 0.0, 0.0],
            [0.684, -0.512, 0.0, 0.0], [0.193, -0.797, 0.0, 0.0],
            [-0.658, -0.539, 0.0, 0.0], [0.423, 0.741, 0.0, 0.0],
            [-0.372, 0.759, 0.0, 0.0], [0.582, -0.604, 0.0, 0.0],
            [-0.795, -0.359, 0.0, 0.0], [0.276, -0.739, 0.0, 0.0],
            [0.913, 0.215, 0.0, 0.0], [-0.184, 0.821, 0.0, 0.0],
            [0.536, 0.660, 0.0, 0.0], [-0.691, 0.513, 0.0, 0.0],
            [0.147, -0.822, 0.0, 0.0], [0.838, 0.288, 0.0, 0.0],
            [-0.461, -0.671, 0.0, 0.0], [0.329, 0.752, 0.0, 0.0],
            [-0.583, 0.614, 0.0, 0.0], [0.756, -0.443, 0.0, 0.0],
            [-0.253, -0.759, 0.0, 0.0], [0.633, 0.584, 0.0, 0.0],
            [-0.718, -0.467, 0.0, 0.0], [0.053, 0.833, 0.0, 0.0],
        ];
        let noise_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSAO Noise"),
            size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &noise_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&noise_data),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(8 * 4 * std::mem::size_of::<f32>() as u32),
                rows_per_image: Some(8),
            },
            wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
        );
        let noise_view = noise_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSAO Noise Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
```

- [ ] **Step 3: Add bind group 2 layout (kernel + noise + sampler) and update pipeline layout**

Replace the existing frame bind group layout code block and pipeline layout. Add kernel BGL after existing `frame_bgl`:

```rust
        let kernel_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Kernel+Noise BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
```

Update pipeline layout from 2 to 3 bind groups:

```rust
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&frame_bgl), Some(&kernel_bgl)],
            immediate_size: 0,
        });
```

Create kernel bind group:

```rust
        let kernel_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Kernel+Noise BG"),
            layout: &kernel_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: kernel_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&noise_sampler),
                },
            ],
        });
```

- [ ] **Step 4: Update GBuffer sampler from Nearest to Linear**

In `resolve()`, change the `SSAO GBuffer Sampler`:

```rust
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSAO GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
```

- [ ] **Step 5: Add fields to `SSAOPass` struct**

Add fields before `enabled: bool`:

```rust
    kernel_buffer: wgpu::Buffer,
    kernel_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    kernel_bind_group_layout: wgpu::BindGroupLayout,
    noise_texture: wgpu::Texture,
    noise_view: wgpu::TextureView,
    noise_sampler: wgpu::Sampler,
```

Add corresponding entries to `Self { }`:

```rust
            kernel_buffer,
            kernel_bind_group,
            kernel_bind_group_layout: kernel_bgl,
            noise_texture,
            noise_view,
            noise_sampler,
```

- [ ] **Step 6: Rewrite WGSL shader — 32-sample loop, noise texture rotation, kernel uniform**

Replace the entire `shader_source` with:

```rust
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

struct SSAOParams {
    radius: f32,
    bias: f32,
    intensity: f32,
    _pad0: f32,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
};

struct SSAOKernel {
    samples: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var gbuffer_depth: texture_depth_2d;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_sampler: sampler;

struct FrameUniforms {
    params: SSAOParams,
    proj_mat: mat4x4<f32>,
    inv_proj_mat: mat4x4<f32>,
    view_mat: mat4x4<f32>,
};

@group(1) @binding(0) var<uniform> frame: FrameUniforms;

@group(2) @binding(0) var<uniform> kernel: SSAOKernel;
@group(2) @binding(1) var noise_tex: texture_2d<f32>;
@group(2) @binding(2) var noise_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let norm_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);

    // Sky check: GBuffer normal is (0,0,0) after clear
    if (norm_sample.r == 0.0 && norm_sample.g == 0.0 && norm_sample.b == 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    // Reconstruct view-space position from depth + UV
    let depth_sample = textureSample(gbuffer_depth, gbuffer_sampler, uv);
    let clip_uv = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let clip_pos = vec4<f32>(clip_uv.x, clip_uv.y, depth_sample, 1.0);
    let view_pos4 = frame.inv_proj_mat * clip_pos;
    let view_pos = view_pos4.xyz / view_pos4.w;
    let frag_view_z = view_pos.z;

    // Normal: read from GBuffer, transform to view space
    let world_N = normalize(norm_sample.xyz * 2.0 - 1.0);
    let view_N4 = frame.view_mat * vec4<f32>(world_N, 0.0);
    let view_N = normalize(view_N4.xyz);

    // Random rotation from 8×8 tiled noise texture
    let noise_uv = uv * frame.params.screen_size / 8.0;
    let rot_sample = textureSample(noise_tex, noise_sampler, noise_uv);
    let rvec = vec3<f32>(rot_sample.r * 2.0 - 1.0, rot_sample.g * 2.0 - 1.0, 0.0);

    // TBN in VIEW space
    let tangent = normalize(rvec - view_N * dot(rvec, view_N));
    let bitangent = cross(view_N, tangent);

    var occlusion: f32 = 0.0;

    // ── 32-sample stratified hemisphere kernel ───
    let view_radius = frame.params.radius;
    let view_bias = frame.params.bias;

    for (var i: u32 = 0u; i < 32u; i = i + 1u) {
        let ks = kernel.samples[i].xyz;
        let sd = tangent * ks.x + bitangent * ks.y + view_N * ks.z;
        let sv = view_pos + sd * view_radius;
        let sc = frame.proj_mat * vec4<f32>(sv, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_depth = textureSample(gbuffer_depth, gbuffer_sampler, suv);
            let occ_clip = vec4<f32>(suv.x * 2.0 - 1.0, 1.0 - suv.y * 2.0, occ_depth, 1.0);
            let occ_view = frame.inv_proj_mat * occ_clip;
            let occ_view_z = occ_view.z / occ_view.w;
            if (occ_view_z >= sv.z + view_bias) {
                let z_delta = abs(frag_view_z - occ_view_z);
                let range_attenuation = 1.0 - smoothstep(0.0, view_radius, z_delta);
                occlusion += range_attenuation;
            }
        }
    }

    // Final AO value
    occlusion = 1.0 - occlusion / 32.0;
    occlusion = pow(occlusion, frame.params.intensity);

    return vec4<f32>(occlusion, 0.0, 0.0, 1.0);
}
"#;
```

- [ ] **Step 7: Update execute() to bind group 2**

In `execute()`, after `pass.set_bind_group(1, &self.frame_bind_group, &[]);`, add:

```rust
        pass.set_bind_group(2, &self.kernel_bind_group, &[]);
```

- [ ] **Step 8: Write unit tests**

In `mod tests`, add two new tests before the closing `}` of the test module:

```rust
    #[test]
    fn kernel_samples_are_normalized() {
        for s in &KERNEL_SAMPLES {
            let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
            assert!((len - 1.0).abs() < 0.001, "kernel sample not normalized: {:?}", s);
        }
    }

    #[test]
    fn noise_texture_is_8x8() {
        let pass = SSAOPass::new(&headless_device());
        assert_eq!(pass.noise_texture.width(), 8);
        assert_eq!(pass.noise_texture.height(), 8);
    }
```

- [ ] **Step 9: Run tests**

```bash
cargo test --lib ssao
```

Expected: all pass. New tests `kernel_samples_are_normalized` and `noise_texture_is_8x8` pass.

- [ ] **Step 10: Run clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 11: Run full workspace tests**

```bash
cargo test --workspace --lib
```

Expected: all tests pass.

- [ ] **Step 12: Commit**

```bash
git add crates/aether-engine/src/renderer/passes/ssao.rs
git commit -m "feat(#88): 32-sample stratified SSAO with 8x8 tiled noise and Linear sampler"
```

---

### Task 2: AOBlur — 5×5 bilateral gaussian

**Files:**
- Modify: `crates/aether-engine/src/renderer/passes/ao_blur.rs`

**Interfaces:**
- Consumes: existing `AOBlurPass`, `BlurParams`
- Produces: 5×5 gaussian kernel weights in WGSL, updated loop bounds, no Rust struct changes (depth_sigma already in BlurParams)

- [ ] **Step 1: Expand WGSL kernel from 3×3 to 5×5**

In the WGSL shader string, replace 3×3 kernel constants with 5×5:

Old:
```wgsl
const KERNEL_SIZE: i32 = 1;
const KERNEL_WEIGHTS: array<f32, 9> = array<f32, 9>(
    0.077847, 0.123317, 0.077847,
    0.123317, 0.195346, 0.123317,
    0.077847, 0.123317, 0.077847,
);
```

New:
```wgsl
const KERNEL_RADIUS: i32 = 2;
const KERNEL_WEIGHTS: array<f32, 25> = array<f32, 25>(
    0.003765, 0.015019, 0.023792, 0.015019, 0.003765,
    0.015019, 0.059912, 0.094907, 0.059912, 0.015019,
    0.023792, 0.094907, 0.150342, 0.094907, 0.023792,
    0.015019, 0.059912, 0.094907, 0.059912, 0.015019,
    0.003765, 0.015019, 0.023792, 0.015019, 0.003765,
);
```

Replace loop bounds in fs_main. Find:

```wgsl
    for (var y: i32 = -KERNEL_SIZE; y <= KERNEL_SIZE; y = y + 1) {
        for (var x: i32 = -KERNEL_SIZE; x <= KERNEL_SIZE; x = x + 1) {
            let idx = (y + KERNEL_SIZE) * 3 + (x + KERNEL_SIZE);
```

Replace with:

```wgsl
    for (var y: i32 = -KERNEL_RADIUS; y <= KERNEL_RADIUS; y = y + 1) {
        for (var x: i32 = -KERNEL_RADIUS; x <= KERNEL_RADIUS; x = x + 1) {
            let idx = (y + KERNEL_RADIUS) * 5 + (x + KERNEL_RADIUS);
```

- [ ] **Step 2: Run tests**

```bash
cargo test --lib ao_blur
```

Expected: existing tests pass.

- [ ] **Step 3: Run full workspace tests**

```bash
cargo test --workspace --lib
```

Expected: all tests pass.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/aether-engine/src/renderer/passes/ao_blur.rs
git commit -m "feat(#88): expand AOBlur to 5x5 bilateral gaussian kernel"
```

---

### Task 3: Visual verification

**Files:**
- Modify: `tests/reference/06_ssao_extreme_mode14.png` (update reference)

- [ ] **Step 1: Build release**

```bash
cargo build --release
```

- [ ] **Step 2: Capture screenshot**

```bash
cargo run --bin aether-launcher --release -- \
  --scene scenes/06_ssao_extreme.ron \
  --screenshot tests/output/06_ssao_extreme_mode14.png \
  --exit-after-frames 60 \
  --no-gui-overlay \
  --freeze-time
```

- [ ] **Step 3: Visual inspection checklist**
  - [ ] Screenshot shows visible AO with reduced noise compared to old reference
  - [ ] Contact shadows present near CubeA/CubeB crevice
  - [ ] No banding artifacts on flat surfaces
  - [ ] No moiré patterns

- [ ] **Step 4: Update reference image**

```bash
cp tests/output/06_ssao_extreme_mode14.png tests/reference/06_ssao_extreme_mode14.png
```

- [ ] **Step 5: Commit**

```bash
git add tests/reference/06_ssao_extreme_mode14.png
git commit -m "test(#88): update SSAO visual reference for 32-sample kernel"
```

- [ ] **Step 6: Write visual verification report**

Save to `tests/reports/2026-06-29-ssao-quality.md`:

```markdown
# Visual Verification Report — SSAO Quality Enhancement

**Issue:** [#88](https://github.com/ruochenhua/AetherEngine/issues/88)
**Date:** 2026-06-29
**Scene:** `scenes/06_ssao_extreme.ron`

## Changes
- SSAO: 16-sample → 32-sample stratified hemisphere kernel
- SSAO: hash-based rotation → 8×8 tiled noise texture
- SSAO: Nearest → Linear GBuffer sampling
- AOBlur: 3×3 → 5×5 bilateral gaussian

## Results
- [ ] Noise visibly reduced
- [ ] Contact shadows tighter
- [ ] No banding or moiré
- [ ] SSIM vs old reference: N/A (quality change, new reference captured)

## Conclusion
Phase 5.5 SSAO quality enhancement complete. Ready to close #88.
```

- [ ] **Step 7: Commit report**

```bash
git add tests/reports/2026-06-29-ssao-quality.md
git commit -m "docs(#88): SSAO quality enhancement visual verification report"
```

---
