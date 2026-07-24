# 当前状态与问题基线

## 已具备能力

- `aether-engine` 提供 ECS、渲染 Pass、场景数据和 Shader 验证。
- `aether-launcher` 负责窗口、设备、管线注册、编辑器和场景启动。
- 天空、大气、云、地形、水面反射和 SSR 已有可运行原型。
- 引擎库测试当前基线为 274 个通过测试（以实际命令输出为准）。
- 云、地形、SSR 场景均可在 release Launcher 中启动并生成截图。

## 关键结构问题

1. `crates/aether-engine/src/renderer/scheduler.rs` 跳过可选 Pass 时，没有统一定义输出资源是清除、保留还是回退；`composite.rs` 仍可能无条件采样输出。
2. `pipeline_builder.rs` 主要按最终 writer 建立依赖，尚未表达资源版本、load/store、格式和尺寸兼容性。
3. `aether-launcher/src/app.rs` 集中了 GPU、场景、编辑器、输入和捕获状态，修改半径过大。
4. `AsyncAssetLoader` 主要停留在测试/基础设施层，尚未成为完整场景加载路径。

## 关键画质问题

- GBuffer、Terrain 和 Water Reflection 的法线变换没有统一使用 inverse-transpose。
- 水面反射使用 `normalize(-in.world_pos)`，隐含相机位于世界原点。
- 大气采用逐像素 raymarch，成本随分辨率增长；天空结果没有直接驱动 IBL。
- 云 alpha 主要由 density 缩放得到，缺少明确 Beer–Lambert 透射和步长控制。

## 不变约束

- GPU 资源采用 wholesale replacement 释放旧分配。
- 跨场景 GPU 缓存必须提供 `clear()`，并在打开/新建场景时调用。
- 所有 Shader 必须进入 CPU naga 验证清单。
- 不使用整文件 checkout 覆盖用户已有修改。
