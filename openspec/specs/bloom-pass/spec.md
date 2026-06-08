# Bloom Pass Specification

## Purpose

实现完整的 multi-pass 屏幕空间辉光（Bloom）效果。

Bloom 通过提取图像中的高亮区域，经降采样高斯模糊后再叠加回原图，模拟真实相机在强光源处的光晕效果。由于 Bloom 必须在线性 HDR 空间计算（tone mapping 会压缩动态范围，导致高亮区域信息丢失），因此 Bloom 链位于 CompositePass 之后、ToneMappingPass 之前。

## Requirements

### Requirement: Bloom extract pass

The system SHALL introduce a `BloomExtractPass` that reads `PostProcessInput` and writes a `BrightTexture` containing only pixels above a configurable luminance threshold.

#### Scenario: High-intensity pixel extraction
- **WHEN** a pixel in `PostProcessInput` has luminance > threshold
- **THEN** `BrightTexture` stores `(color - threshold) * intensity` at that pixel
- **AND** pixels below threshold are black

### Requirement: Multi-level downsample blur chain

The system SHALL implement a 3-level downsample + Gaussian blur chain.

#### Scenario: Downsample pass executes
- **WHEN** `BloomDownsamplePass` receives `BrightTexture`
- **THEN** it produces `BloomMip0` at 1/2 resolution, `BloomMip1` at 1/4, `BloomMip2` at 1/8
- **AND** each level applies a separable Gaussian blur (horizontal + vertical)

### Requirement: Upsample + additive blend chain

The system SHALL implement a 3-level upsample chain with additive blending.

#### Scenario: Upsample pass executes
- **WHEN** `BloomUpsamplePass` receives `BloomMip2`
- **THEN` it upsamples to `BloomMip1` resolution with bicubic/bilinear filtering
- **AND** it additively blends with the existing `BloomMip1`
- **AND** the process repeats up to `BloomTexture` (full resolution)

### Requirement: Bloom composite pass

The system SHALL introduce a `BloomCompositePass` that combines the original HDR image with the blurred bloom texture.

#### Scenario: Final bloom composite
- **WHEN** `BloomCompositePass` reads `PostProcessInput` and `BloomTexture`
- **THEN** it outputs `BloomResult = PostProcessInput + BloomTexture * bloom_intensity`
- **AND** `ToneMappingPass` reads `BloomResult` instead of `PostProcessInput`

### Requirement: ToneMappingPass input update

The system SHALL modify `ToneMappingPass` to read `BloomResult` instead of `PostProcessInput`.

#### Scenario: Tone mapping after bloom
- **WHEN** `ToneMappingPass` executes after `BloomCompositePass`
- **THEN** it reads `BloomResult` (HDR linear, with bloom added)
- **AND** it applies ACES / Reinhard / Off as before
- **AND** it outputs to the swapchain

### Requirement: UI controls for bloom parameters

The system SHALL expose bloom parameters in the Launcher inspector panel.

#### Scenario: User adjusts bloom
- **WHEN** the user changes threshold / intensity / enabled via UI sliders
- **THEN** `BloomExtractPass` and `BloomCompositePass` receive new uniforms on the next frame
- **AND** the bloom effect updates immediately

### Requirement: Bloom can be disabled

The system SHALL support completely disabling bloom with zero GPU overhead from the bloom chain.

#### Scenario: Bloom disabled
- **WHEN** the user unchecks "Bloom Enabled"
- **THEN** `BloomCompositePass` outputs `PostProcessInput` unchanged as `BloomResult`
- **AND** the bloom passes still execute but with zero contribution (or are skipped via early-return)

## Design Decisions

### D1: 6-pass bloom chain

Bloom 由 6 个 Pass 组成，形成完整的降采样-模糊-上采样-合成管线：

```
BloomExtractPass      read PostProcessInput → write BrightTexture
BloomDownsamplePass   read BrightTexture    → write BloomMip0 (1/2)
BloomDownsamplePass   read BloomMip0        → write BloomMip1 (1/4)
BloomDownsamplePass   read BloomMip1        → write BloomMip2 (1/8)
BloomUpsamplePass     read BloomMip2        → write BloomMip1 (blend)
BloomUpsamplePass     read BloomMip1        → write BloomMip0 (blend)
BloomUpsamplePass     read BloomMip0        → write BloomTexture (blend)
BloomCompositePass    read PostProcessInput + BloomTexture → write BloomResult
```

### D2: Shared separable blur shader

降采样和上采样使用同一套 separable Gaussian blur WGSL：先水平 pass、再垂直 pass。每个 `BloomDownsamplePass` 执行一次完整的 separable blur（两个 render pass 或一个 compute pass）。为简化，每个 downsample/upsample 步骤使用单个 render pass + 2D kernel 近似（避免 separable 的复杂度）。

### D3: ToneMappingPass signature update

`ToneMappingPass` 的 signature 从 `read PostProcessInput` 改为 `read BloomResult`。这是 #66 对 #65 的唯一侵入式修改，在 Bloom 开启时提供正确的输入；Bloom 关闭时 `BloomResult` 与 `PostProcessInput` 内容相同。

### D4: Compute vs Render pass

Bloom 链使用 Render Pass（全屏 quad）而非 Compute Shader。原因：
- 当前管线全部是 Render Pass，统一模式降低认知负担
- wgpu 的 render pass 对 2D 图像处理足够高效
- 无需引入 compute pipeline 的额外复杂度

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `renderer/passes/bloom_extract.rs` | **新增** | 阈值提取 Pass |
| `renderer/passes/bloom_downsample.rs` | **新增** | 降采样+模糊 Pass |
| `renderer/passes/bloom_upsample.rs` | **新增** | 上采样+叠加 Pass |
| `renderer/passes/bloom_composite.rs` | **新增** | 最终合成 Pass |
| `renderer/passes/tonemapping.rs` | 修改 | Signature 改为 read BloomResult |
| `renderer/resource.rs` | 修改 | 新增 BrightTexture/BloomMip0/1/2/BloomTexture/BloomResult tags |
| `renderer/passes/mod.rs` | 修改 | 导出新 passes |
| `renderer/scheduler.rs` | 修改 | 新增 bloom setters |
| `aether-launcher/src/main.rs` | 修改 | PipelineBuilder 注册 6 个新 Pass + UI 控件 |

## Dependency Compatibility Matrix

| Crate | 变更 | 风险 | 验证 |
|-------|------|------|------|
| wgpu | 无 | 无 | `cargo check` |
| egui | 无 | 无 | — |

## Acceptance Criteria

- [ ] BloomExtractPass 正确提取高亮区域
- [ ] 3 级降采样链产生 1/2、1/4、1/8 分辨率纹理
- [ ] 3 级上采样链正确叠加模糊结果
- [ ] BloomCompositePass 输出 `BloomResult = HDR + Bloom`
- [ ] ToneMappingPass 读取 `BloomResult` 而非 `PostProcessInput`
- [ ] UI 可调节 threshold / intensity / enabled
- [ ] Bloom 关闭时画面与 #65 完全一致（regression 基线）
- [ ] `cargo test` 全绿
- [ ] `cargo clippy` 无新增警告
- [ ] 视觉测试：`scenes/09_bloom.ron` 高金属度+强光源场景有明显辉光
- [ ] 视觉测试报告归档
