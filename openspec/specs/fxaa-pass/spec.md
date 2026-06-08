# FXAA Pass Specification

## Purpose

实现屏幕空间抗锯齿（FXAA, Fast Approximate Anti-Aliasing），作为后处理链的最后一步。

FXAA 通过检测屏幕空间中的高对比度边缘，沿边缘方向对像素进行智能混合，以消除几何锯齿。与 MSAA 相比，FXAA 的优点是：
- 不需要多采样缓冲区，内存开销极低
- 对 Deferred Shading 友好（MSAA 与 MRT 冲突）
- 单次全屏 pass，性能开销小

FXAA 位于 ToneMappingPass 之后、DebugLinePass 之前，对 tone-mapped 的 LDR 图像做抗锯齿处理。

## Requirements

### Requirement: FXAA Pass implementation

The system SHALL introduce a `FXAAPass` that reads the tone-mapped output and applies FXAA anti-aliasing.

#### Scenario: Edge detection
- **WHEN** the FXAA shader samples the input texture
- **THEN** it computes local luminance contrast
- **AND** if contrast exceeds a threshold, it identifies the edge orientation (horizontal vs vertical)

#### Scenario: Edge blending
- **WHEN** an edge is detected
- **THEN** the shader blends the pixel with its neighbors along the edge direction
- **AND** the blend amount is proportional to the sub-pixel coverage of the edge

#### Scenario: Sub-pixel anti-aliasing
- **WHEN** the edge spans less than one pixel
- **THEN** the shader applies sub-pixel filtering to soften thin lines
- **AND** the result is a smoother edge without visible blur on non-edge pixels

### Requirement: FXAA as final post-process step

The system SHALL place FXAAPass at the end of the post-process chain, before DebugLinePass.

#### Scenario: Post-process order
- **WHEN** the full pipeline executes
- **THEN** the order is: Composite → Bloom → ToneMapping → FXAA → DebugLine
- **AND** FXAA operates on the final color buffer before debug overlay

### Requirement: Configurable quality and threshold

The system SHALL expose FXAA quality presets and edge threshold via UI.

#### Scenario: User adjusts FXAA
- **WHEN** the user changes quality preset (Low / Medium / High) or edge threshold
- **THEN** `FXAAPass` receives new uniforms on the next frame
- **AND** the anti-aliasing strength updates immediately

### Requirement: FXAA can be disabled

The system SHALL support disabling FXAA with a simple copy-to-swapchain fallback.

#### Scenario: FXAA disabled
- **WHEN** the user unchecks "FXAA Enabled"
- **THEN** FXAAPass performs a direct texture copy to the swapchain
- **AND** there is no visual difference other than the absence of anti-aliasing

## Design Decisions

### D1: Standard FXAA 3.11 algorithm

采用 NVIDIA 发布的 FXAA 3.11 算法，这是业界最广泛使用的版本。由于完整算法约 200 行 WGSL，核心逻辑包括：
1. 采样中心像素 + 4 个直接邻居
2. 计算亮度对比度（`luma`）
3. 判断边缘方向（水平/垂直）
4. 沿边缘搜索端点
5. 计算子像素偏移量
6. 最终混合

为控制代码量，使用简化版 FXAA（约 80 行 WGSL），保留核心边缘检测和混合逻辑，省略极端 corner case 处理。

### D2: Luma-based edge detection

使用 `luma = dot(rgb, vec3(0.299, 0.587, 0.114))` 计算亮度，而非 RGB 通道分别检测。这降低了采样次数且效果足够好。

### D3: Quality preset via uniform

```wgsl
struct FXAAUniforms {
    edge_threshold: f32,      // 0.063 (Low) / 0.031 (Med) / 0.016 (High)
    edge_threshold_min: f32,  // 0.0 (Low) / 0.0 (Med) / 0.0 (High)
    subpixel_quality: f32,    // 0.75 (Low) / 0.5 (Med) / 0.25 (High)
    _pad: f32,
};
```

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `renderer/passes/fxaa.rs` | **新增** | FXAAPass 实现 |
| `renderer/passes/mod.rs` | 修改 | `pub mod fxaa;` |
| `renderer/scheduler.rs` | 修改 | 新增 fxaa setters |
| `aether-launcher/src/main.rs` | 修改 | PipelineBuilder 注册 + UI 控件 |

## Dependency Compatibility Matrix

| Crate | 变更 | 风险 | 验证 |
|-------|------|------|------|
| wgpu | 无 | 无 | `cargo check` |
| egui | 无 | 无 | — |

## Acceptance Criteria

- [ ] FXAAPass 正确检测并平滑高对比度边缘
- [ ] 非边缘区域无明显模糊（纹理细节保持清晰）
- [ ] Pipeline 顺序正确：ToneMapping → FXAA → DebugLine
- [ ] UI 可调节 quality preset 和 edge threshold
- [ ] FXAA 关闭时画面无 regression
- [ ] `cargo test` 全绿
- [ ] `cargo clippy` 无新增警告
- [ ] 视觉测试：`scenes/10_fxaa.ron` 高对比度几何边缘明显平滑
- [ ] 视觉测试报告归档
