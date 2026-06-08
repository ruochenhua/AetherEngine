# GPU Instancing — Task Breakdown

## 任务 1：准备工作 + RED

- [ ] 创建 `openspec/specs/gpu-instancing/` 及本文档
- [ ] 验证 `cargo check` 基线
- [ ] 编写 instancing 单元测试骨架：`instance_data_layout`、`batch_grouping`
- [ ] 创建验证场景 `scenes/11_instancing.ron`（1000 个相同 cube，不同位置）

**验收标准**：
- `cargo check` 0 errors
- 测试骨架编译通过
- `11_instancing.ron` 可加载

---

## 任务 2：InstanceData 结构扩展

- [ ] 定义完整 `InstanceData` struct（含 model_matrix + entity_id + padding）
- [ ] derive `Clone, Copy, Debug, Pod, Zeroable`
- [ ] 更新 Extract 阶段：填充完整的 `InstanceData`
- [ ] 添加单元测试：`instance_data_size_and_alignment`

**验收标准**：
- `cargo check` 通过
- `std::mem::size_of::<InstanceData>()` 为 80 字节
- `std::mem::align_of::<InstanceData>()` 为 16

---

## 任务 3：GBufferPass instanced draw

- [ ] 修改 GBufferPass vertex shader：添加 `@location(1)` instance input（model_matrix 作为 4 个 vec4）
- [ ] 修改 GBufferPass pipeline：添加 instance vertex buffer layout
- [ ] 修改 GBufferPass `execute()`：
  - 每 batch 创建/复用 instance buffer
  - `queue.write_buffer` 上传 instance data
  - `pass.set_vertex_buffer(1, instance_buffer)`
  - `pass.draw(0..vertex_count, 0..instance_count)`
- [ ] 更新 GBufferPass 测试

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 gbuffer tests 通过
- 单 instance 场景视觉无 regression

---

## 任务 4：ShadowPass instanced draw

- [ ] 修改 ShadowPass vertex shader：添加 instance model_matrix input
- [ ] 修改 ShadowPass pipeline：添加 instance vertex buffer layout
- [ ] 修改 ShadowPass `execute()`：instanced draw 逻辑
- [ ] 更新 ShadowPass 测试

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 shadow tests 通过
- 阴影在 multi-instance 场景下正确

---

## 任务 5：编译验证

- [ ] `cargo check --all-targets` 0 errors
- [ ] `cargo clippy --all-targets` 无新增 warnings
- [ ] `cargo test -p aether-engine` 全部通过

**验收标准**：
- `cargo check` 0 errors, 0 warnings
- 所有测试绿色

---

## 任务 6：运行时验证 + 视觉测试

- [ ] 运行 Launcher，加载 `11_instancing.ron`
- [ ] 确认 1000 个 cube 全部渲染（无缺失）
- [ ] 确认每个 cube 位置正确（无错位）
- [ ] 确认阴影在 instanced 物体上正确
- [ ] 加载单物体场景（如 `01_deferred.ron`）确认无 regression
- [ ] Debug grid/gizmo 仍正确显示
- [ ] 运行 SMART GATE → MUST_VERIFY
- [ ] Agent 读取截图，判定 PASS/FAIL
- [ ] 生成视觉测试报告 `tests/reports/YYYY-MM-DD-instancing.md`

**验收标准**：
- 1000 物体全部可见且位置正确
- 单物体场景无 regression
- 视觉测试报告已归档
- Issue 可关闭
