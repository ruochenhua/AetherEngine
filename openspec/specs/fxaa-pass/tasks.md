# FXAA Pass — Task Breakdown

## 任务 1：准备工作 + RED

- [ ] 创建 `openspec/specs/fxaa-pass/` 及本文档
- [ ] 验证 `cargo check` 基线（#66 完成后）
- [ ] 编写 FXAAPass 单元测试骨架：signature / init
- [ ] 创建验证场景 `scenes/10_fxaa.ron`（高对比度几何边缘：细网格/尖锐角/薄物体）

**验收标准**：
- `cargo check` 0 errors
- 测试骨架编译通过
- `10_fxaa.ron` 可加载

---

## 任务 2：FXAA Pass 实现

- [ ] 创建 `renderer/passes/fxaa.rs`
- [ ] 全屏 quad shader：简化版 FXAA 算法（~80 行 WGSL）
  - luma 计算 + 4 邻居采样
  - 边缘方向判断（水平/垂直）
  - 沿边缘搜索 + 子像素混合
- [ ] Uniform buffer：edge_threshold, subpixel_quality
- [ ] 实现 Pass trait
- [ ] 添加 signature / init / resolve 测试

**验收标准**：
- `cargo check` 通过
- Signature：reads Swapchain（或前一步的输出纹理），writes Swapchain

---

## 任务 3：管线集成

- [ ] `passes/mod.rs` 导出 `fxaa`
- [ ] `main.rs` PipelineBuilder 在 ToneMappingPass 之后、DebugLinePass 之前注册 FXAAPass
- [ ] `scheduler.rs` 新增 setter：`set_fxaa_enabled`、`set_fxaa_quality`
- [ ] 验证拓扑顺序正确

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 scheduler build test 通过

---

## 任务 4：Launcher UI

- [ ] Launcher egui 面板添加 FXAA 控件：
  - Checkbox: Enabled
  - Dropdown: Quality (Low / Medium / High)
  - Slider: Edge Threshold [0.01, 0.1]

**验收标准**：
- `cargo check` 通过
- Launcher 启动无 panic
- UI 控件可调节，画面实时更新

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

- [ ] 运行 Launcher，加载 `10_fxaa.ron`
- [ ] 开启 FXAA：确认高对比度边缘明显平滑
- [ ] 关闭 FXAA：确认锯齿重现（与 #66 基线一致）
- [ ] 切换 quality preset：Low/Med/High 有细微差异
- [ ] 确认非边缘区域（纹理/文字）无过度模糊
- [ ] Debug grid/gizmo 仍正确显示
- [ ] 运行 SMART GATE → MUST_VERIFY
- [ ] Agent 读取截图，判定 PASS/FAIL
- [ ] 生成视觉测试报告 `tests/reports/YYYY-MM-DD-fxaa.md`

**验收标准**：
- FXAA 开启时锯齿明显减少
- FXAA 关闭时与 #66 无差异
- 非边缘区域无过度模糊
- 视觉测试报告已归档
- Issue 可关闭
