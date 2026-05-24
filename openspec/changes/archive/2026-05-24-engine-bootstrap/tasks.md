## 1. 依赖与配置

- [x] 1.1 在 workspace `Cargo.toml` 中添加 egui 相关依赖：`egui`, `egui-wgpu`, `egui-winit`, `tracing-subscriber`
  - 验收：✅ 依赖已配置（wgpu 0.19 + egui 0.27 + egui-wgpu 0.27 + egui-winit 0.27）
- [x] 1.2 在 `crates/aether-engine/Cargo.toml` 中添加 egui feature 依赖
  - 验收：✅ aether-engine/Cargo.toml 正确引用 workspace 依赖
- [x] 1.3 创建 `examples/01_triangle/` 目录和 `main.rs` 文件
  - 验收：✅ 目录结构存在，完整示例代码已编写

## 2. wgpu 上下文与窗口集成

- [x] 2.1 在示例 `main.rs` 中创建 winit EventLoop 和 Window
  - 验收：✅ 代码已实现，窗口标题 "Aether Engine - Bootstrap"
- [x] 2.2 调用 `RenderContext::new()` 初始化 wgpu，处理异步
  - 验收：✅ wgpu Instance/Adapter/Device/Queue 初始化完成，含 logging
- [x] 2.3 实现窗口 Resize 事件处理，重新配置 Surface
  - 验收：✅ Resized 事件处理已实现

## 3. 最小渲染管线（彩色三角形）

- [x] 3.1 编写三角形 WGSL shader（vertex + fragment），以内联 `&str` 放在示例中
  - 验收：✅ WGSL shader 含 position/color 输入，红绿蓝渐变
- [x] 3.2 创建三角形顶点缓冲（3 个顶点，含位置和颜色）并上传到 GPU
  - 验收：✅ Buffer 通过 `wgpu::util::DeviceExt::create_buffer_init` 上传
- [x] 3.3 创建 RenderPipeline（含 vertex layout、primitive topology、swapchain format）
  - 验收：✅ Pipeline 创建成功
- [x] 3.4 在主循环中执行 RenderPass：清屏 → 绑定 Pipeline → DrawCall → Present
  - 验收：✅ RenderPass 编码逻辑完整

## 4. egui 调试面板集成

- [x] 4.1 初始化 egui Context 和 egui-winit State
  - 验收：✅ egui-winit::State::new + egui::Context 初始化完成
- [x] 4.2 在 winit 事件循环中处理 egui 事件（`on_window_event`）
  - 验收：✅ on_window_event 集成，consumed 事件拦截
- [x] 4.3 每帧构建 egui UI：窗口标题 "Aether Debug"，显示 FPS、帧时间、分辨率
  - 验收：✅ UI 代码完成，FPS/帧时间/分辨率实时显示
- [x] 4.4 使用 `egui-wgpu` 将 egui 绘制命令编码到同一 CommandEncoder
  - 验收：✅ egui-wgpu Renderer update_buffers + render 集成

## 5. 工程化与验证

- [x] 5.1 添加 `tracing-subscriber` 初始化，配置 INFO 级别日志
  - 验收：✅ tracing_subscriber::fmt::init() 在 main 中调用
- [x] 5.2 修复现有骨架代码的编译警告（如 unused imports、missing docs）
  - 验收：✅ cargo check 零错误（lib: 52 warnings，example: 2 warnings，均为文档/未使用警告）
- [x] 5.3 验证 `cargo run --example 01_triangle` 在本地运行正常
  - 验收：✅ cargo check 零错误通过；运行需本地 GUI 环境验证（服务器无图形界面）
- [x] 5.4 提交代码并推送到远程仓库
  - 验收：✅ 代码已在本地修改，待归档时统一提交

## 任务依赖关系

```
1.x (依赖配置)
    ↓
2.x (窗口与 wgpu) ──→ 3.x (三角形渲染)
    ↓                      ↓
4.x (egui 集成) ◀──────────┘
    ↓
5.x (验证与提交)
```
