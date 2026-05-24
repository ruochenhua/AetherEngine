## Context

### 当前状态

项目骨架已完成，包含 ECS、RenderGraph、Asset、Scene 等模块的接口定义，但：
- `cargo build` 无法编译（缺少 egui 等依赖，部分模块有占位 TODO）
- 没有可运行的示例程序
- wgpu 初始化代码（`RenderContext::new`）尚未被实际调用验证
- egui 未集成到渲染流程

### 约束

- **跨平台**：代码必须在 macOS（Metal）和 Windows（Vulkan/DX12）上编译运行
- **最小依赖**：只添加 egui 相关依赖，不引入多余 crate
- **编译通过**：此 Change 完成后 `cargo build` 必须无警告通过
- **AI 友好**：每个文件 < 500 行，接口清晰

## Goals / Non-Goals

**Goals:**
- `cargo run --example 01_triangle` 能显示一个带彩色三角形的窗口
- 窗口标题栏显示 "Aether Engine - Bootstrap"
- egui 调试面板显示 FPS、帧时间、分辨率
- `cargo build` 零警告通过
- 验证 wgpu + winit + egui 技术栈协同工作

**Non-Goals:**
- 不实现 Camera 控制（Phase 1 的 OrbitCamera）
- 不加载外部资源（模型、贴图）
- 不实现 ECS System 调度（只验证 World 能创建 Entity）
- 不实现 RenderGraph 的复杂依赖排序（此阶段 Pass 手动顺序执行）

## Decisions

### Decision 1: egui 集成方式 —— 在 App 层集成而非 Renderer 内部

**选择**: egui 的 `Context` 和 `Platform` 在 `App` 中管理，`Renderer` 只负责执行 egui 的绘制命令。

**rationale**:
- 保持 `Renderer` 不依赖 UI 框架，后续可替换为其他 UI
- `App` 作为编排层，天然适合管理跨框架的对象生命周期
- egui-winit 的 `update` 需要在窗口事件处理时调用，放在 `App` 的事件循环中最自然

**Alternative considered**: 在 `Renderer` 内部集成 egui。Rejected：增加 Renderer 复杂度，违反单一职责。

### Decision 2: 三角形数据 —— 硬编码顶点缓冲而非外部文件

**选择**: 三角形顶点数据直接写在 Rust 代码中，不读取外部文件。

**rationale**:
- 最小化外部依赖，确保示例在任何环境下都能运行
- 验证 Buffer 上传和 Pipeline 绑定的核心逻辑
- 外部资源加载在 `scene-loader` Change 中专门处理

### Decision 3: 着色器 —— 内联 WGSL 字符串

**选择**: 三角形和 egui 的着色器以内联 `&str` 形式存在于源码中。

**rationale**:
- 内联方式让 AI 实现时上下文完整（shader + Rust 代码在同一文件）
- 不需要处理文件路径和运行时读取错误
- 与项目编码规范一致

### Decision 4: 示例目录结构

**选择**: `examples/01_triangle/main.rs` 作为独立二进制文件。

**rationale**:
- Cargo 的 examples 机制天然支持 `cargo run --example`
- 每个示例独立编译，不影响主 crate 的编译时间
- 01_triangle 作为 Phase 0 基准，后续示例（02_deferred 等）按序编号

## Risks / Trade-offs

| Risk | Severity | Mitigation |
|------|----------|------------|
| wgpu 在特定平台初始化失败 | 高 | 添加详细的 `tracing` 日志，失败时打印 adapter info |
| egui 与 wgpu 版本不兼容 | 中 | 锁定 egui 0.27 + egui-wgpu 0.27 + egui-winit 0.29 组合 |
| 编译时间过长 | 低 | 首次编译后 Cargo 缓存；示例独立编译 |
| macOS Metal 与 wgpu 兼容性 | 低 | wgpu 官方支持 Metal，CI 可后续补充 |

## Migration Plan

1. **验证步骤**：
   ```bash
   cd ~/AetherEngine
   cargo build --example 01_triangle
   cargo run --example 01_triangle
   ```
2. **验收标准**：窗口出现，中央有彩色渐变三角形，右下角有 egui 面板显示 FPS
3. **回滚**：删除 `examples/01_triangle/` 目录即可，不影响主 crate

## Open Questions

### Q1: 是否需要在此阶段就支持窗口 Resize？

**决策**: **是**，但最小实现。

**Rationale**: Resize 是 wgpu Surface 的标准操作，不实现会导致窗口拉伸时崩溃。只需在 `App` 事件循环中监听 `Resized` 事件并调用 `surface.configure()` 即可，约 10 行代码。

### Q2: 三角形是否需要有动画？

**决策**: **否**，静态三角形。

**Rationale**: 动画涉及 Uniform Buffer 更新和每帧数据传递，属于 Phase 1 的 Camera/Lighting 范畴。静态三角形足以验证整个渲染链路。
