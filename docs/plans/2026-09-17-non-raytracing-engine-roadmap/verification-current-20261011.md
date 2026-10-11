# 路线当前状态（2026-10-11）

本文是当前待办入口。较早的 10 月 5 日续验收和 T0–T6 审计保留为历史证据，不应直接作为今天的完整状态使用。

## 当前状态

<!-- ROADMAP_STATUS:START -->
**Status synced 2026-10-11.** Automated checks and human acceptance are tracked separately; a capture alone does not approve visual results.

| Task | Implementation / automation / acceptance | Remaining | Evidence |
| --- | --- | --- | --- |
| T0 · Verification foundation | Implemented; automation passed; partially accepted | Review the 25 remaining visual candidates; promote only individually approved references. | [T0 execution package](t0-foundation.html) · [T0 candidate capture report (25 scenes)](t0-visual-candidate-report-20261011.md) |
| T1 · Editor transactions and snapshots | Implemented; automation passed; accepted | No known open items | [T1 execution package](t1-editor.html) |
| T2 · Multiple lights | Implemented; automation passed; partially accepted | Review the three-local-light candidate; low-limit hardware remains covered by a synthetic capability matrix. | [T2 execution package](t2-lighting.html) |
| T3 · Material resolution and PBR consumers | Implemented; automation passed; accepted | No known open items | [T3 execution package](t3-material.html) |
| T4 · Transparency and composite | In progress; automation partial; human review pending | Review T4.1/T4.2 candidates and verify water composition, resize, and scene-switch interaction. | [T4 execution package](t4-transparent.html) |
| T5 · CPU particles | In progress; automation partial; partially accepted | Finish typed runtime statistics, 4K performance delta, and flame/smoke/dust readability review. | [T5 execution package](t5-particles.html) |
| T6 · Physics lifecycle | Implemented; automation passed; human review pending | Manually confirm the actual UI click feel and review the 100-body capture before promoting it from reference-pending. | [T6 execution package](t6-physics.html) |
| T7 · glTF and animation | Implemented; automation passed; accepted | Optional: inspect the Inspector playback controls separately; core T7 is accepted. | [T7 execution record](t7.2-execution.md) |
| T8 · Assets, prefabs, and hot reload | Implemented; automation passed; accepted | No known open items | [T8 completion record](t8-assets-completion.md) |
| T9 · Performance and visibility | Implemented; automation partial; partially accepted; research only; production NO-GO | Rebuild the clean pinned v0.1 paired baseline for T9.1; T9.4 remains research-only and production NO-GO. | [T9.1 execution record](t9.1-execution.md) · [T9.4 decision record](t9.4-execution.md) |
<!-- ROADMAP_STATUS:END -->


## T0 视觉候选采集

本轮已采集原始矩阵剩余 25 个场景，runner 全部正常退出，候选均保留为 `reference-pending`。逐项哈希及采集 provenance 见[跟踪报告](t0-visual-candidate-report-20261011.md)；本机图片和 HTML 详情位于忽略目录 `tests/reports/t0-candidate-final-20261011-62057/`。尚未执行人工批准或 reference promotion。

## T6 本轮 Release 证据

主报告：[HTML](../../../tests/reports/visual-20261011T032426Z-30130/t6_physics_stack_100_seek_10s/report.html) · [physics-runtime.json](../../../tests/reports/visual-20261011T032426Z-30130/t6_physics_stack_100_seek_10s/physics-runtime.json) · [重复运行 JSON](../../../tests/reports/t6-physics-runtime-repeat-20261011-30483/t6_physics_stack_100_seek_10s/physics-runtime.json) · [debug on/off 配对报告](../../../tests/reports/t6-physics-debug-pair-20261011.html)

- `dynamic_body_count=100`，总 body/collider 数各为 101（包含固定地面）。
- body 与 collider handle 创建数各为 101；动态物理状态 SHA-256 在两次 Release 运行中相同。
- debug-off 报告为 0 条 debug 线段；debug-on 报告为 4212 条线段，且 runtime state SHA-256 与 debug-off 完全相同。
- launcher exit 0，进程组清空。此场景的截图仍是 `reference-pending`，没有批准 golden，因此不宣称图像差异通过。

## 质量门

2026-10-11 本轮 `./scripts/verify-ci.sh` 全部通过：fmt、Clippy、报告格式、路线图/视觉审核自动化、Metal/进程 runner 合同、workspace 测试（engine 402 项、launcher 75 项）、module-health（329 个 Rust 文件、0 个超限）以及 workspace Release 构建。Release launcher 加载 `scenes/13_clouds.ron` 并完成 640×360 截图后正常退出，云层首帧可见。

## 建议顺序

1. 先完成人工可操作的候选复核：T2 三局部光、T4.1/T4.2，再逐项处理 T0 剩余 25 张图。
2. 收尾 T5 runtime stats、4K 性能 delta 和效果辨识度；补 T4 水面 resize/场景切换。
3. 重建 T9.1 clean pinned v0.1 paired baseline。
4. 最终按仓库治理规则运行完整 `./scripts/verify-ci.sh`；渲染改动另跑相关场景 regression。

视觉 reference 的人工审核与 promotion 必须逐项进行；候选截图和自动运行成功本身不等于视觉验收通过。
