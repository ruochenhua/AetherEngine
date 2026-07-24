# Phase 5：引擎能力扩展

## 目标

把现有特效原型扩展为能承载更大场景和更多材质类型的运行时。

## GPU 地形

- 将 chunk 可见性、LOD 选择和间接绘制数据放入 GPU。
- 使用屏幕空间误差选择 LOD，避免仅按距离切换。
- 地形资源按摄像机范围流式加载并支持 eviction。
- 保留 CPU 路径作为 debug/reference path。

## 透明物体

- 增加独立透明队列和排序键。
- 明确透明物体与深度、雾、云和 SSR 的合成顺序。
- 为 alpha blend、alpha clip、additive 分别定义材质策略。

## 动画与骨骼

- 骨骼层级和 inverse bind pose。
- GPU skinning buffer。
- Animation clip、采样、混合和状态机。
- 动画资源异步加载和场景级释放。

## 资源流式加载

- Asset handle 稳定且不与 GPU cache entry 混淆。
- 资源拥有引用计数或显式 eviction policy。
- 大资源上传分帧执行，避免单帧尖峰。

## 退出条件

- 地形可在多个 LOD 下稳定运行且无明显裂缝。
- 透明物体不会破坏不透明深度和后处理。
- 动画与资源系统可在场景切换后完全释放。
