# GPU Instancing Specification

## Purpose

将相同 mesh + 材质的多个物体合并为单次 draw call，大幅降低 CPU 提交开销和 GPU draw call 数量。

当前渲染管线中，Extract 阶段已按 `(mesh_handle, material_key)` 分组生成 `RenderBatch`，但每个 batch 通常只包含 1 个 `InstanceData`（因为 Phase 3 场景物体数量少）。Phase 4 引入 Instancing 后，每个 `RenderBatch` 可包含 N 个 instance，通过 `wgpu::RenderPass::draw()` 的 `instance_count` 参数一次性绘制。

Instancing 对以下场景效果显著：
- 粒子系统（数千个相同 billboard）
- 植被（大量相同树木/草丛）
- 建筑构件（重复砖块/瓦片）

## Requirements

### Requirement: InstanceData expansion

The system SHALL expand `InstanceData` to include all per-instance attributes needed for drawing.

#### Scenario: Instance data per entity
- **WHEN** Extract phase processes an entity with `(Transform, MeshHandle, MaterialUniform)`
- **THEN` it produces an `InstanceData` containing:
  - `model_matrix: Mat4` — world transform
  - `entity_id: u32` — for picking feedback（预留）

### Requirement: Instance buffer upload

The system SHALL upload all `InstanceData` for a batch into a GPU buffer before the render pass begins.

#### Scenario: Batch with N instances
- **WHEN** a `RenderBatch` contains N instances
- **THEN** all N `InstanceData` are uploaded to a single `wgpu::Buffer`
- **AND** the buffer is bound as a vertex buffer at slot 1（slot 0 是 mesh 几何）

### Requirement: GBufferPass instanced draw

The system SHALL modify `GBufferPass` to use instanced draw calls.

#### Scenario: Drawing a batch with instances
- **WHEN** `GBufferPass` processes a `RenderBatch` with `instances.len() > 1`
- **THEN** it calls `pass.draw(0..vertex_count, 0..instance_count)`
- **AND** each instance receives its own `model_matrix` and `entity_id` via instance vertex buffer
- **AND** the material uniform is shared across all instances in the batch

### Requirement: ShadowPass instanced draw

The system SHALL modify `ShadowPass` to use the same instancing strategy as `GBufferPass`.

#### Scenario: Shadow rendering with instances
- **WHEN** `ShadowPass` processes a `RenderBatch`
- **THEN** it uses instanced draw with the same instance buffer
- **AND** only the `model_matrix` is needed（shadow 不需要材质）

### Requirement: Single-instance fallback

The system SHALL preserve the existing behavior for batches with a single instance.

#### Scenario: Batch with 1 instance
- **WHEN** a `RenderBatch` contains exactly 1 instance
- **THEN** the draw call still uses `instance_count = 1`
- **AND** the visual result is identical to the pre-instancing implementation

### Requirement: Per-instance material（预留）

The system SHALL reserve the capability for per-instance material variation without changing the instancing architecture.

#### Scenario: Future per-instance material
- **WHEN** a future issue adds per-instance material overrides
- **THEN** only `InstanceData` 和 shader 需要扩展
- **AND** instancing 核心逻辑（buffer upload + draw call）不需要修改

## Design Decisions

### D1: Vertex buffer instancing（非 storage buffer）

使用 vertex buffer instancing 而非 storage buffer：
- 与当前 GBufferPass 的 vertex buffer 架构一致
- 无需修改 bind group 布局
- wgpu 自动处理 instance divisor

Instance vertex buffer layout：
```rust
#[repr(C)]
struct InstanceData {
    model_matrix: [[f32; 4]; 4], // 64 bytes
    entity_id: u32,               // 4 bytes
    _pad: [u32; 3],               // 12 bytes, align to 16
}
// Total: 80 bytes per instance
```

### D2: Pre-upload before render pass

所有 per-instance 数据在 render pass 开始前通过 `queue.write_buffer` 上传，render pass 内只做 `set_vertex_buffer` + `draw`。这是 ADR-0008 中已确认的 best practice（避免 Metal 上跨 draw call 的 uniform buffer 写入不可靠）。

### D3: Shared instance buffer across passes

GBufferPass 和 ShadowPass 共享同一份 instance buffer 数据，但各自管理自己的 GPU buffer（因为两个 pass 的 shader 输入布局不同）。Extract 阶段只产生一份 `Vec<InstanceData>`，各 pass 按需上传。

### D4: Material remains per-batch（非 per-instance）

当前阶段材质仍按 batch 共享。per-instance material 是预留扩展，不在本 issue 实现。这限制了 instancing 的适用场景（相同 mesh + 相同材质），但实现简单且覆盖了最常见的重复物体场景。

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `renderer/extract.rs` | 修改 | `InstanceData` 扩展为完整结构体 |
| `renderer/passes/gbuffer.rs` | 修改 | Draw 循环改为 instanced draw + instance buffer upload |
| `renderer/passes/shadow.rs` | 修改 | Draw 循环改为 instanced draw |
| `renderer/passes/gbuffer.rs shader` | 修改 | Vertex shader 添加 instance input |
| `renderer/passes/shadow.rs shader` | 修改 | Vertex shader 添加 instance input |
| `aether-launcher/src/main.rs` | 可能修改 | Extract 调用点确认 |

### 影响面 grep

```bash
grep -n "draw\|InstanceData\|RenderBatch" crates/aether-engine/src/renderer/passes/gbuffer.rs
grep -n "draw\|InstanceData\|RenderBatch" crates/aether-engine/src/renderer/passes/shadow.rs
grep -n "InstanceData\|extract" crates/aether-engine/src/renderer/
```

## Dependency Compatibility Matrix

| Crate | 变更 | 风险 | 验证 |
|-------|------|------|------|
| wgpu | 无 | 无 | `cargo check` |
| glam | `Mat4` → `[[f32; 4]; 4]` | 低 | `cargo check` |
| bytemuck | InstanceData 需 derive Pod/Zeroable | 低 | `cargo check` |

## Acceptance Criteria

- [ ] `InstanceData` 包含 `model_matrix` + `entity_id`，derive `Pod`/`Zeroable`
- [ ] GBufferPass 对 multi-instance batch 使用 `draw(..., instance_count)`
- [ ] ShadowPass 对 multi-instance batch 使用 `draw(..., instance_count)`
- [ ] 单 instance batch 视觉结果与修改前完全一致
- [ ] `cargo test` 全绿
- [ ] `cargo clippy` 无新增警告
- [ ] 视觉测试：`scenes/11_instancing.ron` 1000 个 cube 渲染正确（无缺失/错位）
- [ ] 视觉测试报告归档
