# Phase 1 执行任务

## V1.1 统一验证脚本

**目标文件：** `scripts/verify-engine.ps1`

实现顺序：

1. 设置 `$ErrorActionPreference = 'Stop'`。
2. 顺序执行 fmt、clippy、engine lib test、launcher release build。
3. 每一步输出阶段名和退出码。
4. 只有全部通过才调用场景截图命令。
5. 将输出保存到 `target/verification/<timestamp>/`，不写入源码目录。

验证：

```text
powershell -File scripts/verify-engine.ps1
```

## V1.2 视觉比较 fail-closed

**修改文件：** `.claude/skills/aether-visual-test/scripts/compare_images.py`

要求：

- PIL、numpy、SSIM 实现任一缺失时返回 2。
- 输入尺寸、通道数或颜色空间不一致时返回 2。
- 指标为 NaN/Inf 时返回 2。
- 所有指标通过才返回 0；任一阈值失败返回 1。
- 输出 JSON，包含 `mae`、`changed_ratio`、`ssim`、`thresholds` 和 `tool_version`。

## V1.3 场景验证

固定三条命令：

```text
target/release/aether-launcher.exe --scene scenes/13_clouds.ron --screenshot target/verification/clouds.png --exit-after-frames 120 --no-gui-overlay
target/release/aether-launcher.exe --scene scenes/08_terrain.ron --screenshot target/verification/terrain.png --exit-after-frames 120 --no-gui-overlay
target/release/aether-launcher.exe --scene scenes/07_ssr_debug.ron --screenshot target/verification/ssr.png --exit-after-frames 120 --no-gui-overlay
```

每条命令必须确认进程退出码为 0，日志中出现 first-frame 完成标记，且输出图片存在且非空。

## V1.4 Merge skill

**修改文件：** `.claude/skills/aether-merge/scripts/aether-merge`

将流程改为：预检查 → 拉取 → 合并 → build → test → 场景验证 → 报告。任何中间失败均保留现场，不自动删除 worktree；只有用户明确允许时才执行清理。
