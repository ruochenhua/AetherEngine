## Why

Aether Engine 当前已完成模块骨架（ECS、RenderGraph、Asset、Scene 等），但尚未有可运行的程序验证整个架构的正确性。需要一个最小可运行的窗口程序来确认 wgpu + winit + hecs + egui 技术栈能够协同工作，并建立后续开发的基准验证机制。

## What Changes

- 创建 `examples/01_triangle/` 最小可运行示例
- 集成 `egui` + `egui-wgpu` + `egui-winit` 调试 UI
- 实现最小 RenderPass：彩色三角形（验证顶点缓冲、Pipeline、SwapChain 全链路）
- 添加 `egui` 调试面板（显示 FPS、帧时间、分辨率）
- 验证并修复现有骨架代码的编译问题
- 添加 `tracing-subscriber` 初始化到示例入口

## Capabilities

### New Capabilities
- `window-bootstrap`: winit 窗口创建与事件循环管理
- `wgpu-context-init`: wgpu Instance / Surface / Device / Queue 初始化
- `triangle-render-pass`: 最小 RenderPass 实现（顶点缓冲 + Pipeline + DrawCall）
- `egui-debug-panel`: egui 集成与调试面板渲染

### Modified Capabilities
- （无现有 spec 需要修改，均为新功能）

## Impact

- **Examples**: 新增 `examples/01_triangle/main.rs`
- **Dependencies**: `Cargo.toml` workspace 添加 `egui`, `egui-wgpu`, `egui-winit`, `tracing-subscriber`
- **Renderer**: `renderer/mod.rs` 需暴露必要接口供示例使用
- **App**: `app.rs` 需支持可选的 egui 集成
- **Build**: 新增 example 构建目标 `cargo run --example 01_triangle`
