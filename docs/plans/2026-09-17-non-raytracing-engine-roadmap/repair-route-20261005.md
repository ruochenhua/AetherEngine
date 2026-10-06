# AetherEngine 路线修复计划（2026-10-05）

阶段检查点 `e7fdd0a` 已推送到 `origin/main`。路线依据[续验收摘要](verification-continuation-20261005.md)和[状态审计](t0-t6-status-audit.md)制定。当前修复工作在本地进行，尚未提交。

## 顺序与验收条件

### 1. T5 粒子开关独立工作

- 让 `--particles` 单独启用粒子合成 pass；透明网格仍由 `--transparent` 控制。
- 固化四种开关组合的回归测试，并给粒子单开场景保留可复跑的 v2 case。
- Release 场景需看见固定时间点的粒子，launcher 自动退出且进程组清空。
- **当前状态：** 本地实现和四组合测试已完成；用户人工确认粒子单开截图符合预期。以当前 Release 重新采集后与已验收截图逐像素一致（SHA-256 `82cb8de8…3947e4`），launcher 正常退出且进程组清空；已显式 promote 为 reference。报告：[已验收 T5 粒子单开](../../../tests/reports/t5-particle-only-accepted-refresh-20261005/t5_particle_fire_particles_only_seek_0p50/report.html)。火焰/烟/尘辨识度及性能统计仍归入 T5 收尾。

### 2. T3.2 材质统一消费路径

- 让活动提取和批次统一消费 `ExtractedMaterial`，收敛 `MaterialUniform` 兼容适配与 fallback。
- 覆盖 inline、资产引用、旧 schema、缺失/损坏依赖的测试；补齐 T3 标准 slice 出口。
- 用材质矩阵 Release 场景核对 fallback 与截图报告；仅对用户已验收的候选显式登记 reference。
- **当前状态：** 新路径、旧路径适配、GBuffer/WaterReflection 消费、sRGB 纹理上传与 ORM swizzle 已接通；engine 398 项和 launcher 67 项测试通过，Release 构建成功。用户已通过活动路径视觉验收，该路径 reference 已显式 promote；旧路径仅作诊断对照，仍为 `reference-pending`。最新报告为 [T3.2 对照验收](../../../tests/reports/t3.2-20261005T151351Z-6964/final-review.html)。后续 Release 复跑与参考图哈希完全一致（SSIM 1.0、MAE 0），报告状态仍是 `needs-review`，因为该 runner 对人工验收条件采取 fail-closed 状态。`verify-ci.sh` 的格式、Clippy、runner 检查及 workspace tests 通过；module-health 仍报 52 项增长违规，超 500 行文件为 0，尚未改写基线。

### 3. T4 透明、Composite 与 RenderGraph 证据

- 补真实 GPU sentinel/readback 与资源依赖 graph artifact；CPU 方程测试只保留为单元覆盖。
- 覆盖透明/水面四种开关组合、窗口 resize 与场景切换，处理水面遮挡构图。
- 每项运行需有 typed 结果、截图、日志和自动清理证据。
- **当前状态：** T4.1 已在生产 Composite shader 上回读四个 GPU sentinel；Release capture 导出 42 条资源边，验证 SSR→VolumetricCloud→GodRay→WaterReflection→Transparent→Water→Composite 顺序及 `TransparentColor` 到 Composite 的读边。报告 [T4.1 验收](../../../tests/reports/t4.1-20261005T155008Z/final-review.html) 仍待人工查看。T4.2 overlap 已在当前 Release 重采，远红/近蓝透明物体重叠、中心叠加物和不透明背景可见，launcher exit 0 且进程组清空；报告 [T4.2 overlap 候选](../../../tests/reports/t4.2-overlap-review-20261005/t4_transparent_overlap/report.html) 等待人工确认，reference 未提升。T4.3 水面构图与 resize/scene-switch 运行交互仍待处理。

### 4. T5 运行统计、性能与视觉收尾

- 完成 typed runtime 粒子统计与 4K 性能差值；粒子开关和 T4 合成路径分别可验证。
- 改善火焰、烟、尘的可辨识度，再运行固定时间点及重复采集。

### 5. T6 物理运行态与交互

- 用 Release launcher 实际运行 100-body 场景，导出 live count、稳定状态 hash、allocation 和 debug JSON。
- 覆盖鼠标 picking 的运行交互及 debug on/off 对模拟状态的隔离，并接入标准 typed slice。

### 6. 视觉失败项与证据不足项

- 按功能分组修复 8 项失败图：SSAO 两项、terrain 两项、Perlin terrain、water、God Rays、terrain/water/cloud 组合。
- 对 10 项证据不足/需改进的场景补构图或运行证据；只把人工明确通过的候选显式 promote。

### 7. T9 与质量门收尾

- 为 T9.1 重建与 pinned source 一致的干净配对基线；保留原基线并记录 source fingerprint。
- T9.4 在生产指标、运行时接线和适用硬件验证完成前维持 NO-GO。
- 分类 module-health 的 52 项增长违规，逐项判断修代码、拆文件或已有债务；不得批量更新 baseline 来转绿。

## 每阶段共同门槛

- 运行改动相关的单元/集成测试、Release 构建和受影响场景回归；最终重新跑完整 `verify-ci.sh`。
- 所有场景采集使用有界自动退出流程，结束后确认 launcher 进程组清空，不留测试窗口。
- 将代码/自动验证状态与人工视觉验收分开记录；reference、module-health baseline 均不自动晋升或改写。
