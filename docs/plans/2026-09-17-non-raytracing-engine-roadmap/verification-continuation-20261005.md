# AetherEngine 路线续验收（2026-10-05）

本报告汇总本轮 T3–T9 的验证、自主视觉检查与当前工作树质量门结果，供统一人工验收。它不表示 Release 全绿；T3.2 活动材质路径已由用户验收并登记 reference，其余候选图未自动升级。

完整可点开的视觉与证据索引：[统一验收 HTML](../../../tests/reports/roadmap-continuation-20261005/final-review.html) · [43 图总览](../../../tests/reports/t4-t6-visual-self-review-20261005-6616/contact-sheet-43.png) · [机器回归汇总](../../../tests/reports/t4-t6-visual-self-review-20261005.html)。

## 视觉自检

43 个场景截图逐张检查：**25 项符合，10 项需更强证据或改进，8 项不符合**。所有新增截图保留 `reference-pending` / `needs-review`，未自动 promote。

- 需要修复的 8 张：SSAO 两项、terrain 两项、Perlin terrain、water、God Rays、terrain/water/cloud 组合。
- 证据不足/需改进的 10 项中，原始矩阵 3 项为 shadow debug、IBL roughness、CSM；新增项目包含 T4 graph/水面组合和 T5 火焰/烟/尘效果。
- 其余视觉自检符合项目验收点。T0 的 `01_deferred` 与 T3.2 活动材质路径是已批准并 promote 的 reference。

## 本轮测试与执行状态

| 范围 | 最新证据 | 结论和未闭合项 |
| --- | --- | --- |
| T3 | 材质筛选 23 项通过；T3.3 Inspector primary 报告通过；T3.2 新增 resolver-backed extraction 与旧路径开关；engine 398 项、launcher 67 项通过 | [T3.2 新旧路径对照](../../../tests/reports/t3.2-20261005T151351Z-6964/final-review.html)：用户已通过活动路径并显式 promote；旧路径保留为 pending 诊断对照。Release 复跑哈希完全相同（SSIM 1.0、MAE 0），报告为 `needs-review`，runner 按人工条件 fail-closed。格式、Clippy、runner 和 workspace tests 通过；module-health 报 52 项增长违规，超 500 行文件为 0，未改基线。 |
| T4 | 16 项透明/Composite 相关测试通过；T4.1 新增生产 Composite GPU sentinel readback 与 Release scheduler graph artifact | [T4.1 报告](../../../tests/reports/t4.1-20261005T155008Z/final-review.html)的 42 条资源边和 pass order 检查通过，launcher exit 0 且进程组清空，截图待人工查看。[T4.2 overlap 候选](../../../tests/reports/t4.2-overlap-review-20261005/t4_transparent_overlap/report.html) 已由当前 Release 采集，远红/近蓝透明叠加、中心物体与不透明背景可见，exit 0 且进程组清空；等待人工确认，reference 未提升。T4.3 水面构图、resize/scene-switch 运行交互仍未完成。 |
| T5 | 粒子库 10 项、runtime integration 16 项通过；独立粒子开关及四种开关组合已接通；粒子单开截图经用户确认符合预期 | 当前 Release 重采与验收图 SHA-256 相同，launcher exit 0 且进程组清空；已 promote reference，报告：[T5 粒子单开验收](../../../tests/reports/t5-particle-only-accepted-refresh-20261005/t5_particle_fire_particles_only_seek_0p50/report.html)。4K 性能差、typed runtime stats 和火焰/烟/尘辨识度仍待处理。 |
| T6 | physics 32 项、100-body fixture parse 1 项通过；5/100-body、ramp、debug on/off 截图完成 | 100-body runtime 的 live count/hash/allocation/debug JSON 未出现在统一 typed artifact；picking 运行交互未验收。 |
| T7 | T7.2、T7.3 exit 0；四姿势采样 hash 不同 | 用户此前已通过动画视觉验收；通用 actor/Inspector 动画控件仍未实现。 |
| T8 | T8.1–T8.4 当前运行全部 exit 0 | 没有 approved goldens，所以仅视为测试与首帧渲染证据。 |
| T9 | T9.1–T9.4 标准入口均运行；T9.2/T9.3 PASS；T9.4 research PASS | T9.1 capture Complete 但 pinned baseline 与当前 dirty source 不一致；T9.4 仍为 NO-GO production。 |

T9 标准入口最初受 WindowsApps 的 `python3` shim 干扰；`scripts/run-slice-tests.sh` 已改为先运行解释器探测。修复后 T9.1–T9.3 均通过当前标准入口重跑。T3.2 已补入统一标准入口并生成配对截图；T4–T6 的其余证据仍需逐 slice 收口。

## 质量门

- `cargo fmt --check`、Clippy、runner/report 格式测试通过。
- `cargo test --workspace` 通过：aether-engine lib 394 项、aether-launcher lib 65 项；其他 workspace targets 和 doc-tests 也通过。
- `cargo build --locked --workspace --release` 通过。
- `verify-module-health.sh` 检出 **52 项 baseline-growth 违规**；检查 325 个 Rust 文件，超 500 行文件为 0。没有修改 baseline。
- 完整 `verify-ci.sh` 因 module-health 返回 exit 1；module-health 之前各阶段均通过。
- T4–T6、T7–T9 场景采集采用自动关闭流程；结束后没有残留 `aether-launcher.exe`。

完整逐图画廊与运行链接保存在本地忽略目录 `tests/reports/roadmap-continuation-20261005/final-review.html`；本文件保存本次阶段结论和后续待办。
