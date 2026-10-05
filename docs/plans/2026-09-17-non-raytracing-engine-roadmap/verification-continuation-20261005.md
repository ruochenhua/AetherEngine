# AetherEngine 路线续验收（2026-10-05）

本报告汇总本轮 T3–T9 的验证、自主视觉检查与当前工作树质量门结果，供统一人工验收。它不表示 Release 全绿，也未把候选图升级为 golden reference。

完整可点开的视觉与证据索引：[统一验收 HTML](../../../tests/reports/roadmap-continuation-20261005/final-review.html) · [43 图总览](../../../tests/reports/t4-t6-visual-self-review-20261005-6616/contact-sheet-43.png) · [机器回归汇总](../../../tests/reports/t4-t6-visual-self-review-20261005.html)。

## 视觉自检

43 个场景截图逐张检查：**25 项符合，10 项需更强证据或改进，8 项不符合**。所有新增截图保留 `reference-pending` / `needs-review`，未自动 promote。

- 需要修复的 8 张：SSAO 两项、terrain 两项、Perlin terrain、water、God Rays、terrain/water/cloud 组合。
- 证据不足/需改进的 10 项中，原始矩阵 3 项为 shadow debug、IBL roughness、CSM；新增项目包含 T4 graph/水面组合和 T5 火焰/烟/尘效果。
- 其余视觉自检符合项目验收点。T0 的 `01_deferred` 是此前唯一已批准并 promote 的 reference。

## 本轮测试与执行状态

| 范围 | 最新证据 | 结论和未闭合项 |
| --- | --- | --- |
| T3 | 材质筛选 23 项通过；T3.3 Inspector primary 报告通过；[T3.2 合同审计](t3.2-contract-gap.html) | T3.2 的活动提取/批次仍直接使用 `MaterialUniform`，未完成统一 `ExtractedMaterial` 消费路径。 |
| T4 | 16 项透明/Composite 相关测试通过；7 个场景成功采集 | 缺 GPU sentinel/readback 和完整 graph dump；水面+透明构图遮挡严重；resize/scene-switch 交互未做。 |
| T5 | 粒子库 10 项、runtime integration 16 项通过；4 个截图完成 | `--particles on` 仍需配合 `--transparent on`；4K 性能差和 typed runtime stats 缺失；火焰、烟、尘视觉较弱。 |
| T6 | physics 32 项、100-body fixture parse 1 项通过；5/100-body、ramp、debug on/off 截图完成 | 100-body runtime 的 live count/hash/allocation/debug JSON 未出现在统一 typed artifact；picking 运行交互未验收。 |
| T7 | T7.2、T7.3 exit 0；四姿势采样 hash 不同 | 用户此前已通过动画视觉验收；通用 actor/Inspector 动画控件仍未实现。 |
| T8 | T8.1–T8.4 当前运行全部 exit 0 | 没有 approved goldens，所以仅视为测试与首帧渲染证据。 |
| T9 | T9.1–T9.4 标准入口均运行；T9.2/T9.3 PASS；T9.4 research PASS | T9.1 capture Complete 但 pinned baseline 与当前 dirty source 不一致；T9.4 仍为 NO-GO production。 |

T9 标准入口最初受 WindowsApps 的 `python3` shim 干扰；`scripts/run-slice-tests.sh` 已改为先运行解释器探测。修复后 T9.1–T9.3 均通过当前标准入口重跑。T3.2、T4–T6 的测试与截图已逐项收集，但 `run-slice-tests.sh` 尚未有统一的逐 slice 入口。

## 质量门

- `cargo fmt --check`、Clippy、runner/report 格式测试通过。
- `cargo test --workspace` 通过：aether-engine lib 394 项、aether-launcher lib 65 项；其他 workspace targets 和 doc-tests 也通过。
- `cargo build --locked --workspace --release` 通过。
- `verify-module-health.sh` 检出 **52 项 baseline-growth 违规**；检查 325 个 Rust 文件，超 500 行文件为 0。没有修改 baseline。
- 完整 `verify-ci.sh` 因 module-health 返回 exit 1；module-health 之前各阶段均通过。
- T4–T6、T7–T9 场景采集采用自动关闭流程；结束后没有残留 `aether-launcher.exe`。

完整逐图画廊与运行链接保存在本地忽略目录 `tests/reports/roadmap-continuation-20261005/final-review.html`；本文件保存本次阶段结论和后续待办。
