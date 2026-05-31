# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

一个基于 **Rust** 和 **wgpu** 构建的现代渲染引擎，用于学习从 Deferred PBR 到实时光线追踪的实时图形技术。

> **这是一个 AI-first 的代码库。** 每一项架构决策——模块边界、接口设计、测试策略、贡献流程——都以 AI Agent 作为主要开发者、人类作为审阅者来优化。详见下方 [🤖 AI 优先设计](#-ai-优先设计)。

## 🌟 特性

- **现代架构**：ECS (hecs) + 类型安全的 Pass 调度（PipelineBuilder / Scheduler）
- **跨平台**：wgpu 自动适配 Vulkan/Metal/DX12
- **延迟着色**：基于 G-Buffer 的 Blinn-Phong PBR，支持分通道调试
- **UE 风格飞行相机**：右键漫游，WASD + QE 移动，滚轮调速
- **调试工具**：世界网格、RGB 三轴指示器、光照分通道可视化
- **AI 优先**：每个模块适配单次 AI 上下文窗口；添加 Pass = 一个文件 + 一行注册
- **测试驱动**：每次改动都走 red-green-refactor；资源连接错误在构建期暴露

## 🚀 快速开始

```bash
# 克隆仓库
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# 构建
cargo build

# 启动 Launcher（推荐入口）
cargo run -p aether-launcher
```

## 🎮 操控

| 按键 | 功能 |
|------|------|
| `鼠标右键` | 切换飞行模式 |
| `W A S D` | 前 / 左 / 后 / 右 |
| `Q` / `E` | 下降 / 上升（世界空间） |
| `鼠标` | 旋转视角（飞行模式） |
| `滚轮` | 调节移动速度 |
| `0` – `5` | 光照调试：完整 / 环境光 / 漫反射 / 高光 / 法线 / NdotL |
| `Esc` | 返回 Launcher 菜单 |

## 🤖 AI 优先设计

Aether Engine 不只是**借助** AI 构建——它是**为 AI** 而设计的。每一项设计选择都从 AI Agent 的能力和局限出发来评估。

### 核心原则

| 原则 | 含义 |
|------|------|
| **单文件模块** | 每个模块 < 500 行。AI 可以在一个上下文窗口中阅读、理解、重新生成一个模块。 |
| **声明式优于命令式** | 管线结构通过 `PipelineBuilder::add(pass)` 声明，而非隐藏在 600 行的渲染循环里。 |
| **类型安全的资源连接** | `ResHandle<GPosition>` vs `ResHandle<GNormal>` —— 编译器在渲染前就能发现纹理语义混用。 |
| **构建时失败** | 缺少资源生产者 → `build()` 时 panic。TDD 第一轮就能抓到。不会出现运行时黑屏调试。 |
| **模板驱动创建** | 添加新 Pass = 复制 `passes/template.rs` → 填写签名 + shader → 在 `build_pipeline()` 中注册一行。 |
| **扁平依赖图** | 没有深层继承。Pass 只依赖 `Pass` trait。System 只依赖 `System` trait。 |
| **人在审阅，AI 在编写** | AI 写 PR；人审阅架构契合度和视觉效果。测试证明代码正确。 |

### 模块依赖图

```
main.rs (薄编排层，~80 行)
  │
  ├── PipelineBuilder ──→ Scheduler ──→ [Passes 按拓扑序执行]
  │     ↑                                    │
  │     └── ShadowPass.init()               │
  │     └── GBufferPass.init()              │
  │     └── LightingPass.init()             │
  │     └── DebugLinePass.init()            │
  │                                          │
  ├── SceneLoader ──→ SceneResources { renderables, lighting }
  ├── FlyCamera ──→ view/proj 矩阵
  ├── InputManager ──→ 键盘/鼠标状态
  └── egui ──→ 调试面板
```

**依赖规则：**
- `main.rs` 依赖所有公开 API —— 但只通过薄编排调用
- Pass 模块仅依赖 `Pass` trait + `wgpu` + 自己的 shader
- 添加 Pass：创建 `passes/new_pass.rs` → 在 `build_pipeline()` 加一行 → 在主循环加一行 setter
- Scheduler、PipelineBuilder、ResourceTable 是**一次写完不再改**的基础设施

### AI 如何添加新 Pass（以 SSAO 为例）

```
1. 复制     passes/template.rs       → passes/ssao.rs
2. 填写     signature()              → reads: GPosition, GNormal; writes: AOTexture
3. 填写     init() / resolve()       → 创建 pipeline + bind groups
4. 填写     execute()                → 录制命令
5. 注册     builder.add(SSAOPass::init(device))  ← 一行
6. 添加     ssao_pass.set_config(...)            ← 主循环中一行
7. 运行测试 → 修复构建期错误 → PR
```

**触及文件：2 个**（新 pass 文件、main.rs）。**需审阅文件：1 个**（新 pass）。

### 开发规范

- **测试先行**：先写失败的测试 → 最小代码通过 → 重构。绝不在实现之前写测试。
- **公开接口测试**：测试通过公开 API 验证行为。绝不测试私有函数。
- **构建期错误优于运行时错误**：优先使用让非法状态不可表达的类型。
- **无隐式耦合**：如果 Pass B 依赖 Pass A 的输出，必须在 `signature()` 中声明。
- **Shader 内联**：WGSL 写在 Rust Pass 文件内。一个文件 = AI 的完整上下文。

## 📁 项目结构

```
├── Cargo.toml
├── crates/
│   ├── aether-engine/          # 引擎库
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ecs/              # ECS (hecs 封装)
│   │       ├── scene/            # 场景加载 + RON 反序列化
│   │       ├── asset/            # 资源管理 + 内置网格注册
│   │       ├── renderer/         # 渲染核心
│   │       │   ├── pass.rs       # Pass trait (signature / init / resolve / execute)
│   │       │   ├── scheduler.rs  # Scheduler + PipelineBuilder
│   │       │   ├── resource.rs   # ResHandle<T> + ResourceTable
│   │       │   ├── context.rs    # wgpu 上下文 + RenderContext
│   │       │   ├── camera.rs     # FlyCamera
│   │       │   └── passes/
│   │       │       ├── template.rs  # AI 复制粘贴模板
│   │       │       ├── gbuffer.rs   # G-Buffer (MRT)
│   │       │       ├── lighting.rs  # 延迟光照
│   │       │       └── debug.rs     # 线段渲染（网格、坐标轴）
│   │       ├── physics/          # 物理系统（预留）
│   │       ├── math.rs
│   │       ├── input.rs
│   │       └── window.rs
│   └── aether-launcher/         # Launcher 程序（薄编排）
├── scenes/                      # .ron 场景文件
├── assets/                      # 网格、贴图、着色器
└── docs/
    └── adr/                     # 架构决策记录
```

## 🏗️ 架构

### 渲染管线

```
PipelineBuilder
  ├── ShadowPass       → writes: ShadowDepth
  ├── GBufferPass      → writes: GPosition, GNormal, GAlbedo, GMaterial, GDepth
  ├── LightingPass     → reads: GPosition, GNormal, GAlbedo, GMaterial, ShadowDepth
  │                        writes: Swapchain
  └── DebugLinePass    → reads: GDepth  → writes: Swapchain (LoadOp::Load)
```

资源连接在构建时类型检查。执行顺序由拓扑排序自动推导。

### 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Pass 调度 | PipelineBuilder + Scheduler | 声明式优于命令式——AI 无需读 main.rs 即可理解管线结构 |
| 资源连接 | `ResHandle<T>` 类型标签 | 编译期安全——AI 无法混淆纹理语义 |
| ECS 库 | `hecs` | API 极简，AI 友好，无宏魔法 |
| 渲染 API | `wgpu` | 单一后端，自动适配 Vulkan/Metal/DX12 |
| 着色器语言 | WGSL | 统一，内联——完整上下文给 AI |
| 场景格式 | RON | Rust 原生，类型安全，AI 生成干净 RON |
| UI | `egui` | 即时模式，易于调试面板 |
| 测试策略 | TDD + 仅公开接口 | AI 先写测试，编译器给反馈，安全重构 |

## 📅 路线图

| 阶段 | 特性 | 状态 |
|------|------|------|
| **Phase 0** | 窗口、三角形、egui、Launcher | ✅ 完成 |
| **Phase 1** | Deferred PBR、飞行相机、调试工具、类型安全调度器、阴影映射 | ✅ 完成 |
| **Phase 2** | IBL、屏幕空间效果（SSAO、SSR） | 🔲 计划中 |
| **Phase 3** | ECS 运行时、射线拾取、变换 Gizmo、编辑器 UI、场景保存 | 🔲 计划中 |
| **Phase 4** | 后处理链、色调映射 | 🔲 计划中 |
| **Phase 5** | 地形 + 大气 + 水体 + 体积云 | 🔲 计划中 |
| **Phase 6** | 光线追踪（Compute + Hybrid） | 🔲 计划中 |

## 📜 许可证

MIT OR Apache-2.0

---

*Aether Engine 是 KongEngine 的精神续作，以 AI-first 架构重新构建。*
