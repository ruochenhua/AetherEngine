# T0–T6 状态检查（初检 2026-10-04；更新至 T1 Revision 4 / T2.3，2026-10-05）

> **后续复查补充：** 最新的 T3–T6 定向测试、T4–T6 七个新场景运行及 43 张画面自检，见[续验收摘要](verification-continuation-20261005.md)；本地逐图报告仍位于忽略目录 `tests/reports/roadmap-continuation-20261005/final-review.html`。本轮 T6 增加并解析了 100-body 场景，截图可见 10×10 阵列；live body count/hash/allocation/debug JSON 与 picking 交互仍未形成完整 primary report。

## 结论

这是 2026-10-04 初检及后续 T0–T2.3 更新的历史记录。T3–T6 当前运行、自检与未闭合项以本文上方的续验收补充和统一验收报告为准；较早的 slice 入口失败记录仍说明这些 milestone 的标准入口尚未全部补齐。

- 输入 commit：`7c6792825add554a8ded1eb16e6355ccda31904f`，dirty=true。
- 验证期间源码 fingerprint 未变：`9b86fa338b1d0b733889a32d505624437466108c19627c2aaba62bb77b9fd765`。
- 初检本地证据：[HTML 报告](../../../tests/reports/t0-t6-audit-20261004T154436Z/report.html)；[结构化结果](../../../tests/reports/t0-t6-audit-20261004T154436Z/audit.json)。T0 更新报告：[T0.1](../../../tests/reports/t0-20261004T164029Z/t0_visual_case_migration/report.html)、[T0.2](../../../tests/reports/t0-20261004T164035Z/t0_runner_process_lifecycle/report.html)、[T0.3](../../../tests/reports/t0-20261004T164045Z/t0_report_compare/report.html)。报告目录被 Git 忽略。
- 初检只新增检查记录并校正计划状态；后续 T2.1/T2.2 增量修改见下方记录。未修改 reference 或 module-health baseline。

## 实测总览

- 执行 `./scripts/verify-ci.sh`：初轮 T8 async loading 测试超时；原测试单独复测通过。设置 `RUST_TEST_THREADS=1` 后完整工作区 493 tests passed、0 failed，3 个 doc tests ignored；fmt/Clippy 通过。
- 质量门仍 exit 1：`verify-module-health.sh` 检出 45 项已有 baseline 增长；316 个 Rust 文件、0 个超过 500 行。完整路径及行数见报告。baseline 未修改。
- 独立 `cargo build --locked --workspace --release` 通过。
- 30 次 release 场景运行均产生截图并正常退出；双太阳场景本应拒绝而实际 exit 0，属于合同失败。
- Windows 上进程租约/报告格式检查因 POSIX 要求跳过；回归入口失败 `UnsupportedProcessIsolation`，不能记录为通过。
- 初检时 22 个 `./scripts/run-slice-tests.sh --slice T0.1 … T6.4` 全部 exit 2，脚本不支持这些 slice；`run-primary-case.sh` 不存在。2026-10-05 T0.1/T0.2/T0.3 已新增标准入口并分别 exit 0；T1–T6 的退出入口仍待补。

## T0 更新验证（2026-10-05）

- 迁移结果：`tests/visual-matrix.json` 是 v2，包含 26 个 case；`tests/visual-matrix.v1.bak` 与 26 行迁移 JSONL 对应一致。文档中的迁移 probe 已按当前矩阵修正为 26（原计划模板的 33 不等于此次实际 v1 输入数量）。
- T0.1：5 项 migration/schema 测试与 `time_control` 9 项测试通过；[aggregate/report](../../../tests/reports/t0-20261004T164029Z/t0_visual_case_migration/aggregate.json)。
- T0.2：4 项 reference/promotion 测试与 11 项 Windows Job Object fake-process 测试通过；[aggregate/report](../../../tests/reports/t0-20261004T164035Z/t0_runner_process_lifecycle/aggregate.json)。
- T0.3：5 项 compare/report 测试通过；[aggregate/report](../../../tests/reports/t0-20261004T164045Z/t0_report_compare/aggregate.json)。三份 primary report 均是确定性 CPU fixture，明确不声称 GPU 验收。
- 全量 `verify-ci.sh` 默认并发运行时有一个 T8 async-load 测试超时；以 `RUST_TEST_THREADS=1` 重跑后，fmt、Clippy、工作区 tests 和 runner tests 通过。脚本最终因 module-health 的 45 项既有 baseline-growth 报告返回失败；未修改 baseline。release launcher 单独构建成功。
- 真实候选总览：[26 case gallery](../../../tests/reports/t0-candidate-review-20261005/report.html)，全部用 Release launcher 按序完成采集。`01_deferred` 为 2560×1440、60 帧、Windows Job Object：首帧 handshake 成功，exit code 0，state 从 `Reserved → Bound → Ready → Terminating → Cleaned`，`group_empty=true`；NVIDIA RTX 3070 Ti Laptop GPU / Vulkan / driver 616.64 provenance 完整，结束后 launcher 进程数为 0。用户已视觉验收并确认通过，已显式 promote 到 reference manifest；其余 25 个候选均 exit 0、group_empty、GPU provenance 完整，仍待人工验收且未 promote。
- 只读旧图筛查：17/25 候选的 MAE 或 changed-pixel fraction 超过旧 matrix 阈值，7/25 在这两项内，1/25 没有旧 PNG。对照图和 MAE/fraction 已嵌入总览；未运行昂贵 SSIM，且旧图 driver/provenance 未知，因此不能把该筛查结果当作 release 判定。

## T2.1 增量验证（2026-10-05）

- 场景解析与 `SceneLoader::build_world` 都拒绝多个方向光，并给出 `MultipleSuns` 与 `lights[index]` 诊断；加载完成前还会检查 prefab 合并后的 ECS 光照，避免绕过场景配置约束。
- `extract_lighting_frame` 现在返回带 entity bits/字段名的错误，拒绝多个 sun、非法颜色/intensity/position/direction、非正 range 和无效 spot cone；局部光继续按 entity bits 稳定排序并截断到 32。非法 ECS world 的渲染提取记录诊断并回到 direction-only 默认光。
- launcher 每帧的 sun 更新改为使用同一份 `OptionalPassData.lighting`，移除了另一处重复光照提取。方向光回归 case 与三局部光/云场景均由 Release launcher 自动退出，launcher 进程数归零。
- T2.1 标准入口通过：[CPU 报告](../../../tests/reports/t2-20261005T032837Z-T2.1.html)。三局部光 candidate：[运行报告](../../../tests/reports/t2-single-sun-20261005/t2_three_local_lights/report.html)；云场景 candidate：[运行报告](../../../tests/reports/t2-clouds-20261005/13_clouds/report.html)。二者仍为 needs-review/reference-pending，未 promote。
- 方向光 reference regression 报告为 `needs-review`；没有人工对照确认前不将截图称为视觉通过。完整质量门仍被 45 项 module-health baseline growth 阻断，baseline 未更新。

## T2.2 增量验证（2026-10-05）

- 冻结 GPU ABI 已落实：`GpuLocalLight` 为 64B、align 16；`GpuLocalLightParams` 为 16B。局部光移到独立 group 4（binding 0 storage、binding 1 params），IBL 继续留在 group 3。
- 设备请求仅在 adapter 支持至少 5 个 bind groups 和 fragment storage 时申请 5 组；低于合同的设备创建 direction-only shader/pipeline，不建局部光 group/buffer。能力矩阵和方向光 fallback 的 WGSL parse 均有自动测试；本机实际 Release 路径使用 RTX 3070 Ti / Vulkan，低规格路径以合成 limits + fallback WGSL 测试覆盖，尚无实机低限 adapter 运行证据。
- T2.2 标准入口通过：[ABI 与 fallback 报告](../../../tests/reports/t2-20261005T035935Z-T2.2.html)。Release 三局部光候选：[场景报告](../../../tests/reports/t2-abi-review-20261005T041353Z/t2_three_local_lights/report.html)；exit code 0、`group_empty=true`、无诊断，截图显示 RGB 三个局部光区。它仍是 `reference-pending` / `needs-review`，等待人工确认，未 promote。
- Release `01_deferred` 回归：[报告](../../../tests/reports/t2-20261005-abi-deferred-final.html)。launcher exit 0、进程组清空；与 reference 的 MAE 0、差异率 0%、SSIM 1.0，但按流程仍标 `needs-review`。
- T2.2 阶段的 `verify-ci.sh`：格式、Clippy、报告/进程检查、workspace 测试通过；`verify-module-health.sh` 当时因 44 项 baseline 增长失败，0 个文件超过 500 行。后续 T2.3 的最新质量门结果见本记录末尾。

## T2.3 增量验证（2026-10-05）

- Atmosphere、Volumetric Cloud、GodRay、Water 的 sun 方向改为从 `LightingFrame.sun` 读取；Cloud/Water 的颜色和强度也读取同一 sun 数据。consumer 测试使用不同于旧 `LightingUniforms` 的 frame 值，确认读取统一来源。
- Point/Spot 编辑器按钮均产生对应 typed operation。`--local-lights on|off` 已接入 CLI，默认开启；关闭时使用方向光 shader 变体，不建局部光 group/buffer，并保留现有全屏 draw。
- 自动覆盖 0/1/3 局部光、32/33 上限、device-limit、WGSL fallback、ABI、CLI 和 UI operation；[T2.1 CPU](../../../tests/reports/t2-final-20261005.html)、[T2.2 ABI/fallback](../../../tests/reports/t2-final-20261005-abi.html)、[T2.3 consumer/UI/kill switch](../../../tests/reports/t2-consumer-review-20261005-t2_lighting_consumers.html) 报告通过。
- `--local-lights off` 的 `01_deferred` Release 对照与已批准 reference 的 MAE 0、差异率 0%、SSIM 1.0，launcher exit 0 且 Windows Job Object 清空；自动报告仍标 `needs-review`。新加的 `t2_local_lights_disabled_compat` case 仍是 `reference-pending`，没有借用或新增 reference。
- 三局部光 Release candidate：[报告与画面](../../../tests/reports/t2-t2.3-review-20261005/t2_three_local_lights/report.html)。exit 0、GPU metadata 完整、`group_empty=true`；运行截图显示 RGB 三份局部贡献与方向光同屏。candidate 未 promote，等待用户人工确认。

## 逐任务结果

| 任务 | 实际状态 | 已验证 | 待补齐 |
| --- | --- | --- | --- |
| T0 | 分片实现、CPU 验证与 26 条真实 GPU 候选采集通过；部分发布验收待补 | 实际 v1 矩阵 26 条迁为 v2；原始 `visual-matrix.v1.bak` 和逐案 migration JSONL 保留；production Rust validator 拒绝旧 schema；T0.1/T0.2/T0.3 标准入口均 exit 0 并生成 aggregate/HTML。T0.1 5 项迁移测试 + `time_control` 9 项通过；T0.2 4 项 reference/promotion 测试 + Windows Job Object fake-process 11 项通过；T0.3 5 项比较/报告测试通过。Release launcher 串行采集全部 26 个 case，均生成 GPU metadata 且进程清零；`01_deferred` 已人工验收并 promote，剩余 25 项仍 pending。 | 完整 `verify-ci.sh` 的代码测试通过，最终被 module-health 45 项既有 baseline-growth 阻断；baseline 未改。候选图库包含新图、旧图和差异图；17 项像素指标漂移仍需判断是否为预期效果，不能自动 promote。待用户逐项审核并对通过项显式 promote；之后才能判定 reference acceptance。旧图 provenance 未知，对照只用于筛查；三个 slice 报告是确定性 CPU fixtures，不构成 GPU 视觉验收或 release 证据。 |
| T1 | T1.1–T1.3 Revision 4 primary 全部通过 | 三个 VisualCase v2 fixture 接入统一 slice runner；事务回滚、schema v2 whitelist、10 轮复制/重命名 replay、通用 Light replay、egui UI event→operation、事务落盘和 undo/redo JSONL 一致性均有 typed probe、HTML、JSON、日志和 1×1 sentinel；三个 slice exit 均为 0。 | T1 fixture 是 CPU/UI-context 自动验收，不是视觉渲染验收。旧 Inspector 属性编辑仍保留 EditorCommand 兼容路径；历史 Released 与本次 Revision 4 自动出口状态分别记录。 |
| T2 | T2.1–T2.3 自动化与 Release 场景运行已闭合，等三局部光画面人工验收 | MultipleSuns/非法 ECS light 拒绝；稳定排序/32 上限；64B align16、16B params、独立 group4；4 个 sun consumer 统一读取 LightingFrame；Point/Spot UI、默认开启与 --local-lights off 已验证；低设备和关闭开关走 direction-only。0/1/3、32/33、ABI、device-limit、WGSL fallback、CLI/UI 自动报告通过；feature-off 回归图与批准 reference 像素一致。 | 三局部光 candidate 等用户人工确认，未 promote；没有实机低限 adapter fallback 运行证据。T0 其余候选仍待逐项人工审核。 |
| T3 | 核心复测通过，消费链接口待补齐 | ResolvedMaterial、颜色空间/纹理缓存键、ORM/normal/emissive、Inspector 原子编辑和 round-trip 测试通过；PBR 材质格画面可见。修正测试筛选名称后 schema 测试通过 1 项。 | extract/batch 仍消费 MaterialUniform，ResolvedMaterial 经 legacy adapter 转换；未找到规划中的 ExtractedMaterial/feature fallback 接口。旧脚本一个 schema filter 实际 0 测试，已单独跑正确名称。参考比较 MAE=1.8251、Diff=1.8126%，SSIM 不可用；不能据 fallback exit 0 判定严格视觉出口通过。T3.1–T3.3 标准入口均 exit 2。 |
| T4 | 渲染可运行，GPU 证据待补齐 | TransparentColor/Composite、透明排序与混合、Water 阶段顺序的 CPU/shader 测试通过；透明 on/off 和 Water×Transparent 四组合均截图并正常退出。 | 四格 sentinel 当前是 CPU 方程测试，本轮未获得真实 GPU probe readback/完整 graph artifact；resize/场景切换未做运行交互验收。T4.1–T4.3 标准入口均 exit 2，typed probes/graph 出口待补齐。 |
| T5 | 粒子可见，开关接线待修正 | 16 个粒子 runtime 集成测试通过，包括同 seed/hash、4096/16384 预算、暂停/重启、回滚。透明与粒子同时启用后火焰、烟、尘、billboard 可见；火焰相同时间重复 PNG SHA256 相等。 | 只开 --particles on 不绘制粒子；当前还需 --transparent on，原因是共享 TransparentPass 仅由透明开关注册。4k 性能 delta、typed 粒子统计/出口未交付。T5.1–T5.3 标准入口均 exit 2。 |
| T6 | 物理核心通过，运行证据待补齐 | Rapier 生命周期、权威 pose、多 collider、100-body/600-step 确定性、无物理不分配、切场景、debug 不改 simulation、raycast filter 等单元测试通过。堆叠 0s→10s 位置变化、10s 重复 PNG 相等；坡道 1s 球体位置变化、debug 轮廓可见。 | 运行堆叠场景只有 5 个 dynamic body，不能代替规划要求的 100-body runtime probe；状态 hash/allocation/physics-debug.json 尚未通过标准运行报告导出；鼠标 picking、运行交互及全量 typed relation 未验收。坡道 10s 球已出镜，不作为球体滚动路径证据。T6.1–T6.4 标准入口均 exit 2。 |

## 比较限制

- T3 参考比较：MAE=1.8250551224、Diff%=1.8126085069、SSIM=null。环境缺 skimage，现有比较器使用 Diff≤2% fallback 返回 exit 0；没有据此批准严格视觉出口。
- 火焰相同 seed/time 与物理堆叠相同 seek 的 PNG SHA256 均相等；只是图像字节关系，状态 hash 测试的通过证据另见 CPU 日志。
- T4 sentinel 方程测试通过，真实 GPU readback/graph 尚未验证。T6 运行截图中的堆叠只有 5 个动态物体，100-body 测试在 CPU fixture 中执行。

## 后续建议顺序

1. T0：先人工审核候选图库中的 25 条 pending reference；仅对明确通过的项补 provenance 并显式 promote。module-health 基线增长另需治理，不能通过更新 baseline 掩盖。
2. T2.3：人工确认[三局部光候选](../../../tests/reports/t2-t2.3-review-20261005/t2_three_local_lights/report.html)；确认后再决定是否将截图 promote 为 reference。随后按 T0 自检结果复核 pending candidates，再继续 T5 的粒子开关接线。
3. T5：解决 particles 开关与透明 pass 的注册依赖；补 4k 性能及 typed runtime 统计。
4. T3：落实统一材质消费接口与 fallback；修旧 schema 测试 filter；按冻结阈值重新比较。
5. T4/T6：补真实 GPU sentinel/graph、100-body runtime/hash/allocation/debug artifacts、运行交互验收。
6. T1：保留历史 Released；将 CPU primary 证据接入当前 typed 出口，再统一状态。

本记录是检查结论，不表示新增用户人工验收；后续修复需按改动范围重新运行治理与运行检查。

## T0 候选画面自检补充（2026-10-05）

- 已逐张检查 25 条未 promote 的 Release GPU 候选截图，并生成[画面自检总览](../../../tests/reports/t0-candidate-review-20261005/self-review.html)；旧图、差异图和候选运行详情仍见[候选总览](../../../tests/reports/t0-candidate-review-20261005/report.html)。
- 自检结论：15 项画面符合当前矩阵验收点、2 项证据不足、8 项不符合。证据不足项为 `04_ibl_debug`（粗糙度反射差异不够直观）和 `10_csm`（主体过小，不能判断级联阴影连续性）。
- 不符合项：`05_ssao_debug_mode14`、`06_ssao_extreme_mode14`、`08_terrain`、`09_terrain`、`12_water`、`14_god_rays`、`16_terrain_perlin`、`20_terrain_water_clouds`。主要现象分别是遮蔽 debug 缺少可读层次、地形发白/缺细节、水面波动不可见、God Rays 不可见，以及组合场景未显示清楚的地形层。
- 自检不是用户确认；25 条候选全部仍为 `needs-review` / `reference-pending`，本轮未 promote、未更改 reference 或 module-health baseline。`01_deferred` 的既有用户验收与 promote 状态不变。
- 以上 GPU 截图来自既有 2026-10-05 Release 采集。运行报告记载候选 launcher exit 0、Windows Job Object 进程组清空；本轮复查时 aether-launcher 进程数为 0。

## 本轮质量门复核（2026-10-05）

- 执行 `RUST_TEST_THREADS=1 ./scripts/verify-ci.sh`：`cargo fmt --check`、Clippy、report-format、Metal runner、Windows process contract 与全部 workspace tests/doc-tests 均通过；完整脚本在 module-health 检查处 exit 1，因此没有执行脚本末尾的 release build 步骤。原始日志：[verify-ci-final.log](../../../tests/reports/t1-complete-20261005/verify-ci-final.log)。
- 独立执行 `cargo build --workspace --release` 通过；launcher 以 `--exit-after-frames 1` 加载 `18_file_mesh.ron`，首帧截图写入后 exit 0 自动退出，复查无残留进程。[截图](../../../tests/reports/t1-complete-20261005/t1-launcher-smoke.png)、[Release build 日志](../../../tests/reports/t1-complete-20261005/release-build.log)。
- 独立复核 `verify-module-health.sh`：45 项既有 baseline-growth 违规，检查 325 个 Rust 文件，0 个超过 500 行。没有修改 baseline。
- 自检 HTML 有 25 张候选卡片；逐张 PNG 路径检查 25/25 存在。引擎 launcher 当前进程数为 0。

## T1 编辑器复核补充（2026-10-05）

- 在 T1 原有 8 项编辑器单测上新增 10 轮复制/重命名 JSONL replay hash 测试。定向测试通过；随后执行 `cargo test -p aether-engine --lib editor::tests`：9 项通过，0 失败。原始输出：[editor-tests.log](../../../tests/reports/t1-current-20261005/editor-tests.log) 与 [ten-replay-test.log](../../../tests/reports/t1-current-20261005/ten-replay-test.log)。
- 更新 `scripts/verify-editor-t1.sh`：按三个验收关注点精确选择全部 9 个单测，并校验每个 selector 至少命中 1 个测试。最新运行分组为事务 2/2、snapshot/validation 2/2、command replay 5/5；HTML：[T1 分组报告](../../../tests/reports/t1-complete-20261005.html)，机器汇总：[aggregate.json](../../../tests/reports/t1-complete-20261005-aggregate.json)。
- 按 Revision 4 的 primary case register，T1.1–T1.3 都是 CPU fixture；所有 fixture 使用 VisualCase v2，并由 slice runner 输出 1×1 RGBA8 sentinel、typed aggregate、HTML 和日志。规范没有要求单独新增 `--headless-fixture` Launcher CLI，也没有要求 T1 必须新增 `.ron` fixture 场景。
- 新增三个 case：`tests/cases/t1_editor_transaction.json`、`tests/cases/t1_snapshot_whitelist.json`、`tests/cases/t1_editor_command_replay.json`，以及 `t1_editor_primary` 集成 fixture 和 `scripts/run-t1-editor-tests.sh` runner。T1.3 新增 egui 指针点击 smoke，验证“Add Point Light to Selected”产出 `EditorOperation::AddComponent`；应用队列通过原子事务保存，并验证 undo 移除日志项、redo 恢复日志项。T1.1–T1.3 standard slice 本次重跑全部 exit 0。最新正式报告：[T1.1](../../../tests/reports/t1-20261005T055322Z-1329/t1_editor_transaction/report.html)、[T1.2](../../../tests/reports/t1-20261005T055328Z-987/t1_snapshot_whitelist/report.html)、[T1.3](../../../tests/reports/t1-20261005T055401Z-1054/t1_editor_command_replay/report.html)。
- fixture sentinel 仅用于报告合同，不作为渲染图或视觉验收；旧 Inspector 属性编辑继续走 legacy `EditorCommand` 兼容路径。T1 primary fixture 与本次自动切片状态已闭合。
- 对新增回归测试、UI 操作持久化和 runner 改动重跑 `RUST_TEST_THREADS=1 ./scripts/verify-ci.sh`：fmt、Clippy、report-format、Metal wrapper、Windows runner contract（11 项）和全 workspace 测试通过（engine lib 392 项）；脚本最终因 45 项 module-health baseline growth exit 1。独立 `cargo build --workspace --release` 与 launcher 首帧运行通过，场景进程自行退出；module-health baseline 未动。全量 CI 日志位于 [verify-ci-final.log](../../../tests/reports/t1-complete-20261005/verify-ci-final.log)。

## 本轮 T2.3 质量门复核（2026-10-05）

- `RUST_TEST_THREADS=1 ./scripts/verify-ci.sh` 中 fmt、Clippy、runner/report checks、workspace tests/doc tests 通过；engine library 394 项、launcher 65 项通过。完整 gate 最终在 `verify-module-health.sh` 处 exit 1，报告 52 项 baseline-growth 违规；检查 325 个 Rust 文件，0 个超过 500 行。未更新 baseline。日志：[verify-ci.log](../../../tests/reports/t2-complete-20261005/verify-ci.log)。
- T2 Release scene runner 自动构建并启动 `scenes/23_three_local_lights.ron`，生成 1280×720 candidate，exit 0、`group_empty=true`、结束后无 launcher 进程；feature-off 已批准场景回归 MAE 0、差异率 0%、SSIM 1.0。两个 GPU 运行窗口均由 Job Object runner 清理。
- 当前三局部光 candidate 尚未由用户人工确认，reference 未 promote。工作树和 module-health baseline 均保留原状。

完整 T0–T9 人工复核入口：[验证总览（2026-10-05）](verification-summary-20261005.md)。
