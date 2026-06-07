# ADR-0008: ECS World + Extract Phase for Rendering and Editing

## Status

Accepted

## Context

Phase 1-2 的渲染管线使用 `Vec<Renderable>` 作为 GPU 数据的唯一来源。
`SceneLoader::build_resources()` 把 RON 反序列化为 `SceneDescription`，再手动组装成
`Vec<Renderable>`，Passes 直接遍历这个 Vec 来 draw。

这个模式在 Phase 1-2 工作良好，但 Phase 3 引入编辑器功能后暴露了三个问题：

1. **编辑操作需要双向同步**。Gizmo 修改物体位置后，必须同时更新 `Vec<Renderable>`
   中对应项的 transform。没有 ECS 的统一数据模型，同步逻辑会散落在各个 editor
   模块里。

2. **Picking 需要独立的数据源**。Ray picking 要做射线-包围盒检测，需要每个物体的
   transform 和 mesh bounds。如果这些数据只存在于 `Vec<Renderable>`，picking 代码
   需要和渲染代码共享数据结构，耦合度高。

3. **扩展性差**。后续添加动画、粒子、instancing 时，`Vec<Renderable>` 的扁平结构
   无法表达组件化关系（比如"这个 entity 有骨骼但那个没有"），每次新增功能都要
   修改 Renderable struct。

`ecs/` 目录下已经有 `hecs` 的薄包装（World、System trait、SystemRegistry），
但 CONTEXT.md 明确写了"ECS — Phase 1 保留但不参与运行时渲染"。Phase 3 需要
让 ECS 从"预留"变成"主力"。

## Decision

让 **ECS World 成为场景数据和编辑器状态的唯一真相源**。渲染不再直接读取
`Vec<Renderable>`，而是通过 **Extract 阶段** 每帧把 ECS 数据提取为 GPU-ready 的
`Vec<RenderBatch>`，渲染 Pass 只读提取后的数据。

### 关键设计点

1. **ECS Component 定义**
   - `Transform`：TRS 分解（translation: Vec3, rotation: Quat, scale: Vec3）。
     保留分解形式是因为 Gizmo 的平移/旋转/缩放操作天然操作 TRS 分量，
     不需要从 Mat4 反分解。
   - `MeshHandle`：指向 GPU mesh 的句柄。
   - `MaterialUniform`：PBR 材质参数（albedo, roughness, metallic）。
   - `Visibility`：是否参与渲染（用于 future culling）。
   - `Selected`：编辑器标记 Component，附加在选中的 entity 上。

2. **Extract 阶段**
   - 每帧运行一次（在 `Scheduler.execute()` 之前）。
   - Query `World` 中所有带 `(Transform, MeshHandle, MaterialUniform, Visibility)` 的 entity。
   - 按 `(mesh_handle, material_key)` 分组，生成 `Vec<RenderBatch>`。
   - 每个 `RenderBatch` 包含 `mesh_handle`、`material_key`、
     `instances: Vec<InstanceData>`。
   - `InstanceData` 目前只存 `translation + rotation + scale + entity_id`（极简），
     为 Phase 4/5 的 instancing 预留接口，但 Phase 3 每个 batch 大概率只有 1 个 instance。

3. **渲染 Pass 适配**
   - `GBufferPass`、`ShadowPass` 不再遍历 `Vec<Renderable>`，改为遍历
     `Vec<RenderBatch>`。
   - 材质绑定保持 per-draw（bind group），不走 instance buffer。
     Phase 4 做 instancing 时再引入 per-instance material index。

4. **编辑器数据流**
   - Picking：直接 query ECS World 中的 `Transform + MeshHandle`，做 CPU 射线-AABB
     检测。命中后给 entity 附加 `Selected` Component。
   - Gizmo：`GizmoSystem` query `(&Transform, &Selected)`，直接修改 `Transform`
     Component。下一帧 Extract 自然读到新数据。
   - Inspector：egui 面板直接 query `(&Transform, &Selected)` 读取数值，
     输入框修改后直接写回 Component。
   - Scene Save：遍历 ECS World，过滤掉 editor-only Component（`Selected`），
     序列化为 `SceneDescription` → RON。

5. **FlyCam 输入调整**
   - 环顾从 `左键按住` 改为 `Alt + 左键按住`。
   - 左键单击 = Picking（选中/取消选中）。
   - 左键拖拽 Gizmo handle = 编辑操作。

## Considered Options

- **ECS 仅限编辑器，渲染保持 Vec<Renderable>**：rejected — 编辑器和渲染之间需要
  双向同步（Gizmo 改 Transform → 同步到 Vec；Picking 需要读 Vec 的数据），
  同步逻辑散落在多个模块，AI 容易写错。

- **Pass 直接 query ECS World（无 Extract）**：rejected — wgpu 的 `RenderPass`
  encoder 有严格的生命周期约束，和 ECS borrow checker 容易冲突。
  Extract 把 ECS 访问和 GPU 命令录制解耦，Pass 代码更纯净。

- **Flat Vec<RenderObject>（不预留 instancing）**：rejected — Phase 4 做 instancing
  时需要重写 Extract 的接口和所有 Pass 的 draw 循环。Batch Groups 的结构现在
  就预留好，后续加 instancing 时只改 InstanceData 字段和 draw 调用，不动架构。

- **Transform 存 Mat4（不存 TRS）**：rejected — Gizmo 操作需要对 TRS 分量单独
  修改（沿 X 轴平移只改 translation.x），存 Mat4 每次都要反分解，精度也有损失。

- **Command Buffer / 事件队列（用于 Gizmo 写回）**：rejected — 不需要 Undo/Redo
  的 Phase 3 下，Command 队列是多一层无收益的抽象。直接写 World 更简单。

## Consequences

- **Pros**：
  - 单一数据源（ECS World），没有同步问题。
  - 编辑器功能（Picking、Gizmo、Inspector）全部走同一套数据模型。
  - 后续扩展（动画、粒子、instancing）遵循同一模式：新增 Component + System。
  - Extract 阶段把 ECS 访问和 GPU 编码解耦，Pass 代码保持纯粹。

- **Cons**：
  - Phase 3 初期需要重构所有 Pass 的 draw 循环（从 `Vec<Renderable>` 改为
    `Vec<RenderBatch>`），影响面覆盖 GBufferPass 和 ShadowPass。
  - Extract 阶段每帧遍历全部 entity，大场景下有 CPU 开销（Phase 5 再优化）。
  - `Selected` 作为 ECS Component 意味着选中状态在序列化时必须显式过滤，
    否则保存的 RON 会包含编辑器状态。
