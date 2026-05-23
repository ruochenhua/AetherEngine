## 1. 依赖与配置

- [ ] 1.1 在 workspace `Cargo.toml` 中添加 egui 相关依赖：`egui`, `egui-wgpu`, `egui-winit`, `tracing-subscriber`
  - 验收：`Cargo.toml` 包含上述依赖，版本与现有 wgpu/winit 兼容
- [ ] 1.2 在 `crates/aether-engine/Cargo.toml` 中添加 egui feature 依赖
  - 验收：`aether-engine/Cargo.toml` 正确引用 workspace 依赖
- [ ] 1.3 创建 `examples/01_triangle/` 目录和 `main.rs` 文件
  - 验收：目录结构存在，`main.rs` 包含 `fn main() {}`

## 2. wgpu 上下文与窗口集成

- [ ] 2.1 在示例 `main.rs` 中创建 winit EventLoop 和 Window
  - 验收：`cargo run --example 01_triangle` 能弹出标题为 "Aether Engine - Bootstrap" 的窗口
- [ ] 2.2 调用 `RenderContext::new()` 初始化 wgpu，处理异步
  - 验收：窗口出现后控制台打印 GPU adapter 信息（如 "GPU Adapter: Apple M..."）
- [ ] 2.3 实现窗口 Resize 事件处理，重新配置 Surface
  - 验收：拖动窗口大小时不崩溃，无 `SurfaceError::Lost`

## 3. 最小渲染管线（彩色三角形）

- [ ] 3.1 编写三角形 WGSL shader（vertex + fragment），以内联 `&str` 放在示例中
  - 验收：shader 代码包含 position 和 color 输入，输出颜色渐变
- [ ] 3.2 创建三角形顶点缓冲（3 个顶点，含位置和颜色）并上传到 GPU
  - 验收：Buffer 创建成功，数据正确（`bytemuck` 转换）
- [ ] 3.3 创建 RenderPipeline（含 vertex layout、primitive topology、swapchain format）
  - 验收：Pipeline 编译无错误
- [ ] 3.4 在主循环中执行 RenderPass：清屏 → 绑定 Pipeline → DrawCall → Present
  - 验收：窗口中央显示彩色渐变三角形

## 4. egui 调试面板集成

- [ ] 4.1 初始化 egui Context 和 egui-winit State
  - 验收：无编译错误
- [ ] 4.2 在 winit 事件循环中处理 egui 事件（`on_window_event`）
  - 验收：鼠标移动/点击时 egui 能响应
- [ ] 4.3 每帧构建 egui UI：窗口标题 "Aether Debug"，显示 FPS、帧时间、分辨率
  - 验收：egui 窗口可见，数据实时更新
- [ ] 4.4 使用 `egui-wgpu` 将 egui 绘制命令编码到同一 CommandEncoder
  - 验收：egui 面板正确叠加在三角形上方，无闪烁

## 5. 工程化与验证

- [ ] 5.1 添加 `tracing-subscriber` 初始化，配置 INFO 级别日志
  - 验收：控制台输出引擎启动日志和帧渲染日志
- [ ] 5.2 修复现有骨架代码的编译警告（如 unused imports、missing docs）
  - 验收：`cargo build` 零警告通过
- [ ] 5.3 验证 `cargo run --example 01_triangle` 在本地运行正常
  - 验收：窗口显示三角形 + egui 面板，关闭窗口后程序正常退出
- [ ] 5.4 提交代码并推送到远程仓库
  - 验收：`git log` 显示 engine-bootstrap 相关提交，GitHub 可见

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
