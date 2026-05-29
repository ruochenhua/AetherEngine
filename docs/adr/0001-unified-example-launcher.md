# ADR-0001: Unified Example Launcher (同进程共享上下文)

## Status
Accepted

## Context

AetherEngine 目前有 3 个分散的 example（`01_triangle`、`02_deferred`、`03_gltf_scene`），每个都是独立的 `cargo run --example` 入口，各自创建独立的 `EventLoop`、`Window` 和 `wgpu::RenderContext`。这带来了几个问题：

1. **切换成本高**：想看不同效果必须退出当前进程、重新编译/启动另一个。
2. **重复样板**：每个 example 都重复写 winit 事件循环、egui 初始化、窗口创建。
3. **阻碍 AI 接入**：AI Agent 无法在一个持续运行的进程内观察和操作引擎状态（ECS World、RenderGraph），因为每次切换 example 进程就销毁了。
4. **维护负担**：修改窗口/输入/egui 的公共逻辑需要改 N 个 example。

用户希望有一个统一的 exe（Launcher）作为"引擎主页"，在各个 example 之间热切换，同时保留 `cargo run --example` 独立运行的能力。

## Decision

采用 **同进程、共享 Window + RenderContext** 的 Launcher 架构：

1. 在 `aether-engine` 中定义一个 `Example` trait，统一定义 example 的生命周期：
   - `init(&mut self, ctx: &RenderContext)` — GPU 资源初始化
   - `update(&mut self, ctx: &RenderContext, dt: f32, input: &InputManager)` — 纯 CPU 逻辑
   - `prepare(&mut self, ctx: &RenderContext)` — GPU 资源上传（buffer write、bind group 重建）
   - `render(&mut self, ctx: &RenderContext, encoder: &mut CommandEncoder, target_view: &TextureView)` — 纯命令录制
   - `ui(&mut self, egui_ctx: &egui::Context)` — 自定义 debug 面板
   - `cleanup(&mut self, ctx: &RenderContext)` — 切换前清理，防止泄漏
   - `resize(&mut self, ctx: &RenderContext, width: u32, height: u32)` — 分辨率变化

2. Launcher 作为新 binary crate（`crates/aether-launcher`），统一持有：
   - 唯一的 `EventLoop` + `Window`（全生命周期不重建）
   - 唯一的 `RenderContext`（`device`/`queue`/`surface` 不重建）
   - 唯一的 egui 基础设施（`egui::Context`、`egui_winit::State`、`egui_wgpu::Renderer`）
   - 统一的 `InputManager`（先让 egui 消费事件，未消费的再喂给 InputManager）

3. Example 核心逻辑迁移到 `aether-engine/src/examples/` 下的模块，实现 `Example` trait。

4. 保留 `aether-engine/examples/` 下的薄 `main.rs`，调用 `StandaloneRunner` 支持独立运行。

## Consequences

### Pros
- **切换瞬时**：共享 RenderContext，切换 example 只需 `cleanup` → `init`，无窗口闪烁。
- **AI 友好**：进程常驻，AI 可持续观察 ECS/RenderGraph 状态。
- **输入仲裁统一**：egui 和 Example 的输入竞争由 Launcher 统一处理，避免逻辑冲突。
- **公共逻辑一处改**：窗口、egui、输入的公共逻辑收归 Launcher，example 只写差异化内容。

### Cons
- **进程级隔离丧失**：一个 example 的 panic 或 GPU OOM 会拖垮整个 Launcher。AetherEngine 作为学习/演示引擎，可接受。
- **Example 必须适配 trait**：`01_triangle` 等手写全套 winit/wgpu 的 example 需要重构。保留了独立运行入口作为补偿。
- **`prepare`/`render` 分离的纪律**：Example 作者必须自觉遵守 `render` 阶段只碰 `CommandEncoder`。通过 code review 和模板示例约束。

## Alternatives considered

- **多进程 Launcher**（Launcher 作为进程管理器，启动/杀死 example 子进程）：example 零改造、完全隔离，但切换有延迟、无法实现跨 example 的 egui 悬浮菜单、AI 需要 IPC。
- **每次切换重建 Window + RenderContext**：保留 example 原样，但切换有 1-2 秒停顿，AI 上下文断裂。被否决。
