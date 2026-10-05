# AetherEngine 验证总览（2026-10-05）

> **续验收补充：** T3–T9 当前复跑、43 张画面自检和最新质量门结果见[续验收摘要](verification-continuation-20261005.md)和本地[统一交互报告](../../../tests/reports/roadmap-continuation-20261005/final-review.html)。此补充更新本页较早的 T3–T6 运行状态：T6 已补 100-body 场景截图，但未补齐统一 typed runtime artifacts；T9.1–T9.3 标准入口修复后已复跑。

## 总结

本轮完成了可复核的 T0 画面自检、T1.1–T1.3 Revision 4 primary fixture、T2.1–T2.3 多光源验证，并汇总 T0–T9 现有验证记录。它是当前工作树的验收状态报告，不是 Release 通过声明。

- 最新 `RUST_TEST_THREADS=1 ./scripts/verify-ci.sh` 中格式、Clippy、workspace 测试和 runner/report 检查通过；engine library 394 项、launcher 65 项通过。最终被 module-health 拦截：**52 项 baseline-growth 违规**。没有更新基线。
- `cargo build --workspace --release` 单独通过。模块检查覆盖 325 个 Rust 文件，**0 个超过 500 行**。
- 当前 T0 25 个待审核画面由我逐张自检：15 项符合画面标准、2 项证据不足、8 项不符合；没有修改 reference，也没有 promote。
- T1 编辑器模块单测 **9/9 通过**；Revision 4 的三个 VisualCase v2 fixture 和 T1.3 egui 点击→operation→事务保存→undo/redo replay-log smoke 全部通过。三个标准 slice exit 0。T1 fixture 规范不要求新增 `--headless-fixture` CLI。
- Release launcher 用 `--exit-after-frames 1` 打开 `18_file_mesh.ron` 到首帧，截图已保存，进程以 exit 0 自行退出；进程复查为 0，没有留下测试窗口。[启动截图](../../../tests/reports/t1-complete-20261005/t1-launcher-smoke.png)

## 当前状态矩阵

| 任务 | 自动验证与人工状态 | 未闭合事项 |
| --- | --- | --- |
| T0 | Foundation slices 通过；`01_deferred` 已人工验收并 promote。25 张候选自检：15 pass、2 证据不足、8 fail；用户已确认当前验收包通过。 | 其余候选的机器状态仍为 `needs-review/reference-pending`；需逐项确认后才可显式 promote。8 项画面问题和 2 项证据不足见 [T0 自检总览](../../../tests/reports/t0-candidate-review-20261005/self-review.html)。 |
| T1 | 库测试 9/9；T1.1–T1.3 VisualCase fixture 全部通过，含 egui UI event→operation、事务提交及撤销/重做日志一致性。正式 runner 为每项输出 typed JSON、HTML 和 1×1 sentinel。 | fixture 属于 CPU/UI-context 自动验收，不是视觉渲染证据；旧 Inspector 属性编辑保留 legacy `EditorCommand` 兼容路径。详见 [T0–T6 状态审计](t0-t6-status-audit.md) 和最新 [T1.1](../../../tests/reports/t1-20261005T055322Z-1329/t1_editor_transaction/report.html)、[T1.2](../../../tests/reports/t1-20261005T055328Z-987/t1_snapshot_whitelist/report.html)、[T1.3](../../../tests/reports/t1-20261005T055401Z-1054/t1_editor_command_replay/report.html)。启动截图只证明 Release 首帧能运行，不是 T1 的视觉验收图。 |
| T2 | T2.1–T2.3 自动验证通过；4 个 sun consumer 使用 LightingFrame；Point/Spot UI 和默认开启的 `--local-lights on|off` 开关通过；feature-off Release 对照与已批准图像逐像素一致。 | 三局部光 Release candidate 等待人工确认；未 promote reference。低限 fallback 由合成设备能力和 WGSL 测试覆盖，暂无实机低限 adapter。 |
| T3 | 材质 schema/resolver、颜色空间/缓存键、Inspector round-trip 等 CPU 测试通过；PBR 材质格可见。 | `ResolvedMaterial` 仍经 legacy adapter；统一消费/fallback 接口未完成。参考比较依赖 Diff fallback（SSIM 不可用），不能记为严格视觉通过；标准 T3 exits 未闭合。 |
| T4 | 透明排序、资源阶段、混合和 Water 顺序的 CPU/shader 检查通过；四种 on/off 组合已截图。 | sentinel 仍为 CPU 方程测试，缺真实 GPU readback/完整 graph artifact；resize、场景切换的运行交互验收缺失。 |
| T5 | 粒子 runtime CPU 测试通过；透明与粒子同时开启时能看到火焰、烟尘和 billboard，同时间重复 PNG 一致。 | `--particles on` 单独不会绘制；还依赖 `--transparent on`，共享 pass 注册被透明开关控制。4k 性能 delta 与 typed runtime 统计未完成。 |
| T6 | Rapier 生命周期、确定性、raycast/debug 隔离等核心单测通过；已有堆叠/坡道截图运行正常。 | 运行场景只有 5 个 dynamic body；100-body runtime、状态 hash/allocation/debug JSON、picking 交互和标准 typed exits 未完成。 |
| T7 | glTF 动画画面已由用户人工通过；固定姿势 hash、GBuffer/Shadow/WaterReflection 证据存在。 | Inspector 控件仍需单独手动交互检查；通用 scene config 尚不能创建任意蒙皮动画 actor。 |
| T8 | T8.1–T8.4 归档为完成；4 个 primary 报告均通过，T8.2–T8.4 有截图且 launcher 自动退出。 | 这些截图没有批准的 golden reference，不能作为像素比较通过。 |
| T9 | T9.1 pinned v0.1 visual/profile、T9.2 与 T9.3 已由用户人工验收；T9.4 报告和合成 tile 示意已通过人工审阅。 | T9.1 GPU 时间/显存不可用；T9.2 是当前源码 profile；T9.3 尚缺 clean pinned v0.1 配对基线。T9.4 **仅研究用途，生产 NO-GO**。 |

## T0 画面自检

自检只判断候选图是否清楚展示对应验收点，不代替用户逐项批准 reference。符合项集中在多物体、阴影、天空/云、纹理水、PBR 材质、局部光和 SSR；需要重拍或修复的主要是 SSAO debug、纯地形材质、水面波动、God Rays、组合地形/水/云，以及对象过小的 CSM 和反射粗糙度区分。

完整逐图说明和 PNG 链接位于 [self-review.html](../../../tests/reports/t0-candidate-review-20261005/self-review.html)，原始 candidate provenance 位于 [候选报告](../../../tests/reports/t0-candidate-review-20261005/report.html)。25/25 张 PNG 均存在；候选采集进程记录为 `group_empty=true`，本轮复查 launcher 进程数为 0。

## 其他 milestone 画面自检

- **T4**：on/off 截图能区分透明面和 Water 层；water+transparent 组合的大片半透明平面遮住场景中部。截图能作运行证据，但不能证明 RenderGraph 读写顺序；GPU sentinel/readback 仍缺。
- **T5**：fire candidate 里粒子集中成较窄的橙色柱，billboard case 能清楚看到多个彩色面片。功能可见，但艺术效果简单；particles 单独开关不绘制的问题仍是实际功能缺口。
- **T6**：堆叠画面可见少量方块和球体，坡道图能看到球在坡面附近；debug-on 的轮廓线清楚、debug-off 无轮廓，二图状态一致。它们没有覆盖计划要求的 100-body runtime。
- **T7**：青色 bind pose 与橙色抬臂 pose 差异清楚，水面倒影可见；这张图直观展示动画采样结果，通用 scene loader 范围仍有限。
- **T8**：reload 场景中的室内材质、球和立方体可见；它们是成功渲染截图，没有配置 golden image，因此不构成像素回归通过。
- **T9**：T9.1 的 1k 方块网格清楚；T9.2 的 1k/10k 截图构图接近，sidecar 显示 10k 中 62 visible、9938 culled；T9.3 baseline/feature 画面相同，符合配对等价目标。T9.4 的输出是合成 tile 示意，不是引擎场景截图。

抽查用图均来自已保存报告： [T7](../../../tests/reports/t7-manual-review-20261003/t7_gltf_animation_playback/report.html)、[T8.2](../../../tests/reports/t8-20261001T063913Z-1027/t8_material_reload_lkg/report.html)、[T9.3](../../../docs/plans/2026-09-17-non-raytracing-engine-roadmap/t9.3-execution.md)。T4–T6 的完整场景组见 [T0–T6 审计报告](../../../tests/reports/t0-t6-audit-20261004T154436Z/report.html)。

## 质量门

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --check` | 通过 |
| Clippy | 通过 |
| 报告格式 / Metal wrapper / Windows runner contract | 通过 |
| `cargo test --workspace` | 通过；engine library 394 项、launcher 65 项通过，其他 workspace test targets 与 doc tests 也通过 |
| `verify-module-health.sh` | **失败：52 项 baseline-growth 违规**；325 个 Rust 文件，0 个超过 500 行；baseline 未改 |
| `cargo build --workspace --release` | 单独复跑通过 |
| launcher 首帧与进程清理 | Release 场景截图成功；exit 0 自动退出，复查为 0 个存留进程 |

因此完整 `verify-ci.sh` 仍为失败状态，不能报告为全绿。失败项没有被基线更新掩盖。

T1 gate 日志：[verify-ci-final.log](../../../tests/reports/t1-complete-20261005/verify-ci-final.log)；本轮最新 gate 日志：[verify-ci.log](../../../tests/reports/t2-complete-20261005/verify-ci.log)；T2 Release 构建日志：[release-build.log](../../../tests/reports/t2-complete-20261005-release-build.log)。

## 人工验收入口

1. 先看 [T0 逐图自检](../../../tests/reports/t0-candidate-review-20261005/self-review.html)，对 8 张不符合和 2 张证据不足图确认后续修复范围；候选 reference 仍未 promote。
2. T1–T6 的分项状态、证据限制和后续顺序见 [T0–T6 状态审计](t0-t6-status-audit.md)。T1 最新分组报告：[HTML](../../../tests/reports/t1-complete-20261005.html) · [机器汇总](../../../tests/reports/t1-complete-20261005-aggregate.json) · [完整单测日志](../../../tests/reports/t1-current-20261005/editor-tests.log)。
3. T7/T8/T9 的既有执行记录： [T7.2](t7.2-execution.md)、[T8 归档](t8-assets-completion.md)、[T9.1](t9.1-execution.md)、[T9.2](t9.2-execution.md)、[T9.3](t9.3-execution.md)、[T9.4](t9.4-execution.md)。

## 本轮范围与工作树

本轮补齐 T1.3 的 UI 操作接线与持久化撤销/重做验证，完成 T2.3 consumer/开关接线和自动验证，增加对应候选画面；没有 promote 新 reference，也没有修改 module-health baseline。工作树原有的大量未提交修改仍在；报告按 `7c67928` 为 HEAD、dirty working tree 的状态解释，不将旧 commit hash 当作完整源代码快照。
