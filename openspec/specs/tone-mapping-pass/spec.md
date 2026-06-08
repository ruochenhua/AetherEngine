# Tone Mapping Pass Specification

## Purpose

从 LightingPass 中提取 tone mapping，成为独立的、可配置算法的 PostProcess Pass。

当前 LightingPass 内嵌了简化 Reinhard（`color / (color + 1.0)`）。这带来两个问题：
1. **后续效果无法在线性 HDR 空间处理** — Bloom 需要在 tone mapping 之前提取高亮区域，否则辉光会被提前压缩
2. **算法不可切换** — 用户无法对比不同 tone mapping 的效果

将 tone mapping 后移，使 LightingPass 输出线性 HDR，CompositePass 输出到中间纹理，ToneMappingPass 负责最终的 HDR→LDR 映射。

## Requirements

### Requirement: Linear HDR output from LightingPass

The system SHALL remove tone mapping from the LightingPass shader so it outputs linear HDR `SceneColor`.

#### Scenario: LightingPass executes
- **WHEN** the LightingPass fragment shader computes the final lit color
- **THEN** it outputs `vec4<f32>(final_color, 1.0)` without any tone mapping curve
- **AND** the `SceneColor` texture remains `Rgba16Float`

### Requirement: CompositePass writes to intermediate texture

The system SHALL modify CompositePass to write to a transient `PostProcessInput` texture instead of directly to the swapchain.

#### Scenario: CompositePass executes
- **WHEN** CompositePass blends `SceneColor` with `ReflectionTexture`
- **THEN** it writes the result to `PostProcessInput` (Rgba16Float)
- **AND** it no longer directly renders to the swapchain

### Requirement: ToneMappingPass performs HDR→LDR mapping

The system SHALL introduce a `ToneMappingPass` that reads `PostProcessInput` and outputs to the swapchain with a configurable tone mapping curve.

#### Scenario: ToneMappingPass with ACES
- **WHEN** the algorithm is set to ACES
- **THEN** the pass applies Stephen Hill's fitted ACES tone mapping curve
- **AND** highlights are smoothly compressed without clipping artifacts

#### Scenario: ToneMappingPass with Reinhard
- **WHEN** the algorithm is set to Reinhard
- **THEN** the pass applies `color / (color + 1.0)` per channel
- **AND** the result matches the previous LightingPass output (visual regression baseline)

#### Scenario: ToneMappingPass Off
- **WHEN** the algorithm is set to Off
- **THEN** the pass performs simple linear clamp to `[0, 1]`
- **AND** the result may show clipping in over-bright areas (useful for HDR debugging)

### Requirement: Algorithm switching via UI

The system SHALL expose a tone mapping algorithm selector in the Launcher inspector panel.

#### Scenario: User switches algorithm
- **WHEN** the user selects ACES / Reinhard / Off from the dropdown
- **THEN** `ToneMappingPass` receives the new setting on the next frame
- **AND** the visual result updates immediately

### Requirement: Debug line overlay preserved

The system SHALL ensure DebugLinePass still renders on top of the tone-mapped output.

#### Scenario: Debug lines with tone mapping
- **WHEN** DebugLinePass executes after ToneMappingPass
- **THEN** it uses `LoadOp::Load` on the swapchain
- **AND** grid/gizmo lines appear correctly over the tone-mapped scene

## Design Decisions

### D1: PostProcessInput resource tag

新增 `PostProcessInput` tag，格式 `Rgba16Float`，与 SceneColor 同格式。CompositePass 改为 write PostProcessInput，ToneMappingPass read PostProcessInput。

```rust
pub enum PostProcessInput {}
impl ResourceTag for PostProcessInput {}
```

### D2: ACES implementation

使用 Stephen Hill 的简化 ACES 拟合（Naughty Dog 版本），避免完整的 3×3 矩阵变换：

```wgsl
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}
```

### D3: Reinhard as baseline

Reinhard 实现为 `color / (color + 1.0)`，与当前 LightingPass 行为一致。视觉验证时以 Reinhard 为 regression baseline。

### D4: Uniform-driven algorithm selection

算法选择通过 uniform buffer 传入 shader，避免重建 pipeline：

```wgsl
struct ToneMappingUniforms {
    algorithm: u32, // 0=Off, 1=Reinhard, 2=ACES
    _pad: vec3<u32>,
};
```

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `renderer/passes/lighting.rs` | 修改 | Shader 移除 tone mapping 段落（约 5 行） |
| `renderer/passes/composite.rs` | 修改 | 增加 `write::<PostProcessInput>`，修改 execute 输出目标 |
| `renderer/passes/tonemapping.rs` | **新增** | ToneMappingPass 完整实现 |
| `renderer/resource.rs` | 修改 | 新增 `PostProcessInput` tag |
| `renderer/passes/mod.rs` | 修改 | `pub mod tonemapping;` |
| `renderer/scheduler.rs` | 修改 | 新增 `set_tone_mapping_algorithm()` setter |
| `aether-launcher/src/main.rs` | 修改 | PipelineBuilder 注册 Pass + egui UI 控件 |

### 影响面 grep

```bash
# 确认 CompositePass 直接写 swapchain 的位置
grep -n "swapchain\|Swapchain\|surface_view" crates/aether-engine/src/renderer/passes/composite.rs
grep -n "tone\|Tone" crates/aether-engine/src/renderer/passes/lighting.rs
```

## Dependency Compatibility Matrix

| Crate | 变更 | 风险 | 验证 |
|-------|------|------|------|
| wgpu | 无 API 变更 | 无 | `cargo check` |
| egui | 无 | 无 | — |
| glam | 无 | 无 | — |

## Acceptance Criteria

- [ ] LightingPass shader 不执行 tone mapping，输出线性 HDR
- [ ] CompositePass 声明 `write::<PostProcessInput>("post_process_input", Rgba16Float)`
- [ ] ToneMappingPass 声明 `read::<PostProcessInput>("post_process_input")` + `write::<Swapchain>("swapchain", Bgra8UnormSrgb)`
- [ ] PipelineBuilder 拓扑排序正确（CompositePass → ToneMappingPass → DebugLinePass）
- [ ] ACES / Reinhard / Off 三种算法可通过 UI 下拉框切换
- [ ] DebugLinePass 仍正确叠加（swapchain LoadOp::Load）
- [ ] `cargo test` 全绿（含新增 signature/init 测试）
- [ ] `cargo clippy` 无新增警告
- [ ] 视觉测试：`scenes/08_tonemapping.ron` 高曝光场景下 ACES 暗部细节优于 Reinhard
- [ ] 视觉测试报告归档到 `tests/reports/`
