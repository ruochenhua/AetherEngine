# 视觉验证工作流 (Visual Test Workflow)

## 定位

这是 Aether Engine **TDD 闭环的最后一步**。代码编译通过、单元测试 green 不代表任务完成——渲染结果必须经视觉验证后方可进入重构/合并阶段。

## 定义完成 (Definition of Done)

一个渲染特性/issue 只有在满足以下条件时才算完成：

1. **RED** — 失败的测试/场景描述已写好
2. **GREEN** — 代码实现通过所有单元测试 (`cargo test`)
3. **VISUAL VERIFY** — 截图与参考图对比通过，Agent 目视检查 PASS
4. **REFACTOR** — 在视觉确认无误后进行重构
5. **报告归档** — 测试报告已写入 `tests/reports/`

> **没有 Visual Verify 的 green 是伪 green。**

## Issue 生命周期中的验证节点

```
[Open] → [In Progress] → [Code Complete] → [Needs Visual Verify] → [Verified] → [Closed]
                                    ↑                           ↓
                                    └──────── Regression ───────┘
```

### Label 定义

| Label | 含义 | 谁操作 |
|---|---|---|
| `needs-visual-verify` | 代码已完成，等待截图验证 | 研发 Agent 在自测后添加 |
| `visual-verified` | 截图对比 + Agent 检查均通过 | QA Agent 验证后添加 |
| `visual-regression` | 与参考图差异超出阈值 | QA Agent 发现后添加 |

### 检查清单 (Issue 关闭前必须勾选)

Issue body 或评论中必须包含：

```markdown
## Verification Checklist

- [ ] 单元测试通过 (`cargo test`)
- [ ] 场景截图已捕获 (`tests/output/`)
- [ ] 与参考图对比通过 (SSIM ≥ 0.95 或 Agent 判定 PASS)
- [ ] 测试报告已生成 (`tests/reports/`)
- [ ] 参考图已更新（如视觉变更是预期的）
```

## TDD 五步循环（含智能门控）

传统 TDD 是 RED → GREEN → REFACTOR。在本项目中扩展为：

```
RED  →  GREEN  →  SMART GATE  →  VISUAL VERIFY  →  REFACTOR
         ↑___________________________________________|
                        (发现 visual bug 时回退)
```

### 各阶段职责

**RED**
- 写场景 RON 文件描述期望的视觉效果
- 写单元测试（signature、pipeline build、headless device）

**GREEN**
- 实现最小代码使编译通过 + 单元测试通过
- **不要在此阶段重构**

**SMART GATE**（新增）
- Agent 运行 `scripts/should-verify-visual.py` 分析本次变更
- **MUST_VERIFY** → 继续执行 VISUAL VERIFY
- **SHOULD_VERIFY** → 时间允许时执行，至少跑关键场景
- **NO_VERIFY** → 直接跳到 REFACTOR（或关闭 issue）

为什么需要 SMART GATE？
- 重构 ECS 调度器 → 无需截图
- 优化 uniform buffer 上传 → 无需截图
- 但**任何 `passes/` 目录内的改动**（哪怕只是重命名变量）都视为 MUST_VERIFY，因为像素级输出对代码结构极度敏感

**VISUAL VERIFY**
- 运行 `cargo run --bin aether-launcher -- --scene ... --screenshot ...`
- 对比 `tests/reference/` 下的黄金图（如存在）
- Agent 读取截图，对照 PRD  acceptance criteria 判定 PASS/FAIL
- 将结果写入 `tests/reports/YYYY-MM-DD-scene-name.md`

**REFACTOR**
- 仅在 VISUAL VERIFY 为 PASS（或 SMART GATE 判定 NO_VERIFY）时进行
- 重构后必须**重新执行 SMART GATE → VISUAL VERIFY**

## QA Agent 触发条件

以下场景**必须**调用 `aether-visual-test` 技能：

1. **每月里程碑验证** — "验证本月/Phase X 的工作"
2. **Issue 标记 `needs-visual-verify`** — 看到此 label 的 issue
3. **PRD 验收** — PRD 中明确提到视觉效果时
4. **重构后** — 任何涉及 `passes/`、`scheduler.rs`、`lighting.rs` 的修改
5. **参考图缺失** — 新场景首次需要生成参考图

### 但先过 SMART GATE

在调用技能之前，Agent **必须先运行**：

```bash
python3 scripts/should-verify-visual.py --since HEAD~1
```

- 返回 `MUST_VERIFY` → 执行完整视觉验证流程
- 返回 `SHOULD_VERIFY` → 执行关键场景验证（至少跑直接相关的场景）
- 返回 `NO_VERIFY` → 跳过视觉验证，只需 `cargo test`

**例外**：即使 SMART GATE 返回 NO_VERIFY，如果 issue 带有 `needs-visual-verify` label，仍以 label 为准执行验证。

## 报告模板

QA Agent 验证后应在 `tests/reports/` 生成报告：

```markdown
# Visual Test Report — 2026-06-06

## Scene: 05_ssao_debug

### Config
- Scene: `scenes/05_ssao_debug.ron`
- Debug mode: 14 (SSAO)
- Frames: 120
- Features: SSAO=ON, Shadow=ON, IBL=ON, SSR=OFF

### Metrics
| Metric | Value | Threshold | Status |
|---|---|---|---|
| SSIM | 0.9712 | ≥ 0.95 | ✅ PASS |
| MAE | 2.34 | < 10 | ✅ PASS |
| Diff % | 0.12% | < 1% | ✅ PASS |

### Agent Inspection
- ✅ SSAO 遮蔽效果在物体接触处可见
- ✅ 无明显的噪点或光环 artifact
- ⚠️ 远景略有模糊（在预期范围内）

### Verdict
**PASS** — 可以合并

### Artifacts
- Output: `tests/output/05_ssao_debug.png`
- Reference: `tests/reference/05_ssao_debug.png`
```

## 常见问题

**Q: 没有参考图的新场景怎么办？**
A: 首次验证时 Agent 直接做目视检查，若 PASS 则将截图提升为参考图：
```bash
cp tests/output/XX_name.png tests/reference/XX_name.png
git add tests/reference/XX_name.png
```

**Q: 刻意改变视觉效果后参考图失效？**
A: 在 issue 中注明"预期视觉变更"，验证通过后由 Agent 直接更新参考图。

**Q: 如何在 CI 中运行？**
A: 当前需要 GPU surface，无头 CI 需要后续接入 `wgpu` 的 offscreen 模式。目前以本地 Agent 验证为主。
