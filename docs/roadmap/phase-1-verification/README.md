# Phase 1：可信验证链

## 目标

使格式、静态检查、单元测试、Launcher 启动和视觉回归形成强制门禁。

## 验证层级

1. 静态：fmt、clippy、Shader naga manifest。
2. CPU：`cargo test -p aether-engine --lib`。
3. 启动：release Launcher 首帧无 panic。
4. 场景：云、地形、SSR 各自生成截图。
5. 视觉：SSIM/MAE 依赖缺失时必须失败，不能跳过比较。

## 任务

### V1.1 统一本地门禁

在 `scripts/` 增加单一入口，按上述顺序执行并在任一步失败时停止；输出机器可读的阶段结果。

### V1.2 视觉比较 fail-closed

修改 `.claude/skills/aether-visual-test/scripts/compare_images.py`：指标库导入失败、图片尺寸不同、NaN、空图片都返回非零；固定依赖版本并把阈值写入配置。

### V1.3 场景基线

固定 `scenes/13_clouds.ron`、`08_terrain.ron`、`07_ssr_debug.ron` 的 120 帧截图流程，记录 GPU、分辨率、驱动和阈值元数据。

### V1.4 合并门禁

重写 `.claude/skills/aether-merge/scripts/aether-merge` 的失败路径：拉取失败立即退出；合并后执行测试和场景验证；只有确认 worktree 干净且没有用户修改时才允许清理。

## 退出条件

四条全局命令通过；三张基准截图可重复生成；视觉工具依赖缺失会明确失败；合并失败不会丢失分支或 worktree。
