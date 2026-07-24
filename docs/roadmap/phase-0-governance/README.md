# Phase 0：工程治理与文档收敛

## 目标

让 README、CONTEXT、ADR、Roadmap、AI 技能文件和实际代码描述同一套架构。

## 任务

### G0.1 文档事实表

维护一份能力表，逐项标记 `Implemented`、`Prototype`、`Planned`，并以代码、测试或场景作为证据。至少覆盖异步资产加载、Terrain LOD、Atmosphere、Cloud、SSR、Water Reflection 和 Editor。

### G0.2 ADR 对齐

审核 `docs/adr/ADR-0003*`、`ADR-0009*`、`ADR-0010*`、`ADR-0011*`。对已被代码推翻的决策新增 superseded 状态和迁移说明，不删除历史记录。

### G0.3 AI 指导文件

修正 `AGENTS.md`、`CLAUDE.md`、`.claude/skills/aether-merge/` 和 `.claude/skills/aether-visual-test/` 中的路径、测试数量、完成状态和验证命令。

## 退出条件

- 文档不再宣称未集成的功能已经完成。
- 每个架构结论能链接到实现或测试。
- AI 合并和视觉测试技能的命令在干净环境中可执行。
