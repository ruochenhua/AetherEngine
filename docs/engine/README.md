# Aether Engine 文档

这里按功能介绍引擎目前的组成、使用方式和可直接打开的示例场景。项目仍在开发中；文档描述的是当前仓库中的实现，不代表所有功能都已达到生产级质量。

## 从这里开始

- [架构与代码结构](architecture.md)：Workspace、核心数据流、ECS 与渲染调度。
- [渲染功能](rendering.md)：材质、光照、阴影、屏幕空间效果、环境与后处理。
- [编辑器与场景操作](editor.md)：场景浏览、层级、Inspector、变换编辑和场景保存。
- [物理与粒子](simulation.md)：固定步长物理、播放控制、碰撞体调试和粒子。
- [场景与资源](scenes-assets.md)：RON 场景、模型和纹理资源的组织方式。

## 快速打开示例

在仓库根目录运行：

```bash
cargo run -p aether-launcher -- --scene scenes/24_t3_pbr_material_grid.ron
```

更多专项场景可在各功能文档中找到。视觉回归流程见[视觉测试指南](../agents/visual-test-workflow.md)，开发优先级见[路线图](../plans/2026-09-17-non-raytracing-engine-roadmap/index.html)。
