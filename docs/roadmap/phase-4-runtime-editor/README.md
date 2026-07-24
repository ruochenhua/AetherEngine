# Phase 4：运行时与编辑器架构

## 目标

降低 Launcher 状态耦合，让资产加载、渲染帧和编辑器行为拥有清晰边界。

## 目标模块

```text
App
├── GpuContext
├── SceneRuntime
├── RenderRuntime
├── EditorRuntime
├── AssetRuntime
├── InputRuntime
└── CaptureRuntime
```

## 任务

### E4.1 App 状态拆分

从 `crates/aether-launcher/src/app.rs` 提取按生命周期和所有权分组的状态；每个子状态只暴露必要方法，不允许编辑器直接修改 GPU 资源表。

### E4.2 Frame 数据收敛

将 `RenderFrame` 分成 camera、lighting、environment、temporal、debug 五类稳定数据；FrameConfig 只保留跨 Pass 的配置，不承载临时 GPU 对象。

### E4.3 异步资产加载

把 `AsyncAssetLoader` 接入真实场景加载：请求返回 handle，主线程轮询状态，失败返回诊断，GPU 上传在明确的 prepare 阶段完成。

### E4.4 材质系统

建立材质描述、纹理句柄、默认 fallback、shader permutation key 和 GPU bind group 生命周期。

### E4.5 编辑器可靠性

加入保存失败反馈、撤销/重做、资源引用诊断、场景切换清理和渲染 debug view。

## 退出条件

- 场景加载不阻塞渲染线程。
- 任一资源加载失败都能显示可定位错误。
- 编辑器状态变化不会绕过 ECS/资源系统直接修改 GPU 状态。
- `App` 可按子系统单独测试。
