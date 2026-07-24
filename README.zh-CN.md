# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

一个基于 **Rust** 和 **wgpu** 构建的现代渲染引擎，用于学习从 Deferred PBR 到实时光线追踪的实时图形技术。

> **这是一个 AI-first 的代码库。** 每一项架构决策——模块边界、接口设计、测试策略、贡献流程——都以 AI Agent 作为主要开发者、人类作为审阅者来优化。详见下方 [🤖 AI 优先设计](#-ai-优先设计)。

## 🌟 特性

- **现代架构**：ECS (hecs) + 类型安全的 Pass 调度（PipelineBuilder / Scheduler）
- **跨平台**：wgpu 自动适配 Vulkan/Metal/DX12
- **延迟着色**：基于 G-Buffer 的 Cook-Torrance PBR（GGX NDF + Smith G + Schlick Fresnel）
- **UE 风格飞行相机**：Alt + 左键拖拽环视，WASD + QE 移动，滚轮调速
- **调试工具**：世界网格、RGB 三轴指示器、光照分通道可视化
- **场景编辑器**：鼠标点击拾取物体、变换 Gizmo（平移 / 旋转 / 缩放）、场景层级面板、属性检查器（位置 / 旋转 / 缩放 / 材质）、撤销/重做、删除、打开/导入/保存场景（RON）、全屏视口切换
- **AI 优先**：单职责模块（纯逻辑代码约 600 行为健康区，内联 shader 不计行数）；添加 Pass = 一个文件 + 一行注册
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
| `Alt + 左键拖拽` | 环视视角（偏航 / 俯仰） |
| `左键点击` | 在视口中拾取物体（`Shift`/`Ctrl` + 点击加选） |
| `左键拖拽` Gizmo 手柄 | 平移 / 旋转 / 缩放选中物体 |
| `Delete` | 删除选中物体 |
| `W A S D` | 前 / 左 / 后 / 右 |
| `Q` / `E` | 下降 / 上升（世界空间） |
| `滚轮` | 调节移动速度 |
| `0` – `9` | 光照调试：完整 / 环境光 / 漫反射 / 高光 / 法线 / NdotL / 阴影 / 直接光 / IBL / Alpha(P) |
| `F1` – `F5` | 更多调试视图：Alpha(N) / NDC / EnvFix / VDir / SSAO |
| `F6` | 循环切换 SSR 调试模式 |
| `F7` | CSM 阴影调试视图 |
| `⛶ 全屏` | 切换全屏视口（隐藏侧边面板） |

> **注意：** 当 egui 输入框拥有键盘焦点时，调试热键（`0`–`9`、`F1`–`F7`）会自动被屏蔽，避免在编辑数值时意外切换渲染模式。

## 🤖 AI 优先设计

Aether Engine 不只是**借助** AI 构建——它是**为 AI** 而设计的。每一项设计选择都从 AI Agent 的能力和局限出发来评估。

### 核心原则

| 原则 | 含义 |
|------|------|
| **单职责模块** | 一个文件一个职责。纯逻辑代码约 600 行为健康区（内联 shader 不计行数）；超过约 800 行且能识别出第二个独立职责时才拆分。 |
| **声明式优于命令式** | 管线结构通过 `PipelineBuilder::add(pass)` 声明，而非隐藏在 600 行的渲染循环里。 |
| **类型安全的资源连接** | `ResHandle<GPosition>` vs `ResHandle<GNormal>` —— 编译器在渲染前就能发现纹理语义混用。 |
| **构建时失败** | 缺少资源生产者 → `build()` 时 panic。TDD 第一轮就能抓到。不会出现运行时黑屏调试。 |
| **模板驱动创建** | 添加新 Pass = 复制 `passes/template.rs` → 填写签名 + shader → 在 `build_pipeline()` 中注册一行。 |
| **扁平依赖图** | 没有深层继承。Pass 只依赖 `Pass` trait。System 只依赖 `System` trait。 |
| **人在审阅，AI 在编写** | AI 写 PR；人审阅架构契合度和视觉效果。测试证明代码正确。 |

### 模块依赖图

```
main.rs（~7 行）──→ app.rs（Launcher 编排：事件循环 + 编辑器 UI）
  │
  ├── pipeline::build_pipeline() ──→ PipelineBuilder ──→ Scheduler ──→ [Passes 按拓扑序执行]
  ├── SceneLoader ──→ RON 场景 → ECS World 实体 + LightingUniforms
  ├── Extract ──→ 每帧从 ECS World 提取 RenderBatch
  ├── FlyCamera ──→ view/proj 矩阵
  ├── InputManager ──→ 键盘/鼠标状态
  └── egui ──→ 编辑器 UI + 调试面板
```

**依赖规则：**
- Launcher（`main.rs` + `app.rs`）依赖所有公开 API —— 但只通过薄编排调用
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
│   │       ├── scene/            # 场景加载 + RON 序列化/反序列化
│   │       ├── asset/            # 资源管理 + 内置网格注册
│   │       ├── terrain/          # Chunked LOD 地形几何 + 材质
│   │       ├── clouds/           # 体积云程序化噪声纹理
│   │       ├── renderer/         # 渲染核心
│   │       │   ├── pass.rs            # Pass trait (signature / init / resolve / execute)
│   │       │   ├── pipeline_builder.rs# PipelineBuilder + 拓扑排序
│   │       │   ├── scheduler.rs       # Scheduler（执行调度 + resize 重建）
│   │       │   ├── resource.rs        # ResHandle<T> + 资源标签
│   │       │   ├── resource_table.rs  # ResourceTable（瞬态纹理）
│   │       │   ├── frame.rs           # RenderFrame 每帧数据
│   │       │   ├── extract.rs         # ECS → RenderBatch 提取
│   │       │   ├── context.rs         # wgpu 上下文 + RenderContext
│   │       │   ├── camera.rs          # FlyCamera
│   │       │   ├── ibl/               # IBL 预计算 + Skybox
│   │       │   └── passes/            # 全部 Render Pass（见下方渲染管线）
│   │       │       ├── template.rs          # AI 复制粘贴模板
│   │       │       ├── shadow.rs            # 级联阴影贴图
│   │       │       ├── gbuffer.rs           # G-Buffer (MRT)
│   │       │       ├── terrain/             # 地形写入 G-Buffer（可选）
│   │       │       ├── ssao.rs, ao_blur.rs  # SSAO + 位置感知模糊（半分辨率）
│   │       │       ├── lighting/            # 延迟光照
│   │       │       ├── atmosphere.rs        # 解析式天空
│   │       │       ├── volumetric_cloud/    # 体积云 ray marching
│   │       │       ├── ssr/                 # 屏幕空间反射
│   │       │       ├── god_ray.rs           # God Ray
│   │       │       ├── water_reflection.rs  # 水面平面反射
│   │       │       ├── water/               # 前向水面 Pass
│   │       │       ├── composite.rs         # 合并不透明 + 水 + 云 + God Ray
│   │       │       ├── bloom/               # Bloom mip 链
│   │       │       ├── tone_mapping.rs      # HDR → LDR
│   │       │       ├── fxaa.rs              # FXAA 输出到 Swapchain
│   │       │       └── debug.rs             # 线段渲染（网格、Gizmo）
│   │       ├── physics/          # 物理系统（预留）
│   │       ├── math.rs
│   │       └── input.rs
│   └── aether-launcher/         # Launcher 程序（薄编排）
│       └── src/
│           ├── main.rs           # 入口（~7 行）
│           ├── app.rs + app/     # 事件循环、UI、编辑器交互
│           ├── inspector/        # Inspector 面板
│           └── pipeline.rs       # build_pipeline() + 截图工具
├── scenes/                      # .ron 场景文件
├── assets/                      # 网格、贴图、着色器
└── docs/
    └── adr/                     # 架构决策记录
```

## 🏗️ 架构

### 渲染管线

```
build_pipeline() (crates/aether-launcher/src/pipeline.rs) → PipelineBuilder
  ├── ShadowPass           → writes: ShadowDepth (cascade array)
  ├── GBufferPass          → writes: GPosition, GNormal, GAlbedo, GMaterial, GDepth
  ├── TerrainPass          → writes: same G-buffer targets (terrain merged into the G-buffer)
  │                          optional, when scene has terrain
  ├── SSAOPass             → reads: GDepth, GNormal → writes: AOTexture (half-res)
  ├── AOBlurPass           → reads: AOTexture, GPosition → writes: AOTextureBlurred (half-res)
  │                          (SSAO + AOBlur are runtime-toggleable via ssao_enabled)
  ├── LightingPass         → reads: GPosition, GNormal, GAlbedo, GMaterial, ShadowDepth, AOTextureBlurred
  │                        → writes: SceneColor (HDR)
  ├── AtmospherePass       → reads: GDepth → writes: SceneColor (sky drawn after lighting)
  │                          optional, when scene has atmosphere
  ├── VolumetricCloudPass  → reads: GDepth → writes: CloudColor
  │                          optional, when scene has clouds
  ├── SSRPass              → reads: GPosition, GNormal, GMaterial, GDepth, SceneColor
  │                        → writes: SsrTraceResult (half-res) + ReflectionTexture
  ├── GodRayPass           → reads: GDepth → writes: GodRayColor
  │                          optional, when scene has god rays
  ├── WaterReflectionPass  → writes: WaterReflectionColor, WaterReflectionDepth (planar reflection)
  │                          optional, when scene has water with reflection enabled
  ├── WaterPass            → reads: SceneColor, ReflectionTexture, WaterReflectionColor, GDepth
  │                        → writes: WaterColor     optional, when scene has water
  ├── CompositePass        → reads: SceneColor, ReflectionTexture, WaterColor, CloudColor, GodRayColor, G-buffer
  │                        → writes: PostProcessInput
  ├── BloomPass            → reads: PostProcessInput → writes: BloomResult
  ├── ToneMappingPass      → reads: BloomResult → writes: FxaaInput (HDR → LDR)
  ├── FXAAPass             → reads: FxaaInput → writes: Swapchain
  └── DebugLinePass        → reads: GDepth → writes: Swapchain (grid/gizmo lines over the final image)
```

资源连接在构建时做类型检查。Scheduler 对这些签名做拓扑排序推导执行顺序——没有未决依赖的 Pass（如 WaterReflectionPass、VolumetricCloudPass、GodRayPass）可能早于其注册位置执行。可选 Pass 每帧通过 `should_run()` 跳过；TerrainPass 仅在场景包含地形时注册（ADR-0010）。DebugLinePass 最后注册，作为 `Swapchain` 的最后一个顺序写入者，始终最后执行。

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
| **Phase 1** | Deferred PBR、飞行相机、调试工具、类型安全调度器、阴影映射、IBL + Skybox | ✅ 完成 |
| **Phase 2** | 屏幕空间效果（SSAO、SSR） | ✅ 完成 |
| **Phase 3** | ECS 运行时、射线拾取、变换 Gizmo、编辑器 UI、场景保存/加载、撤销/重做、删除 | ✅ 完成 |
| **Phase 4** | 后处理链、色调映射、Bloom、FXAA、GPU Instancing | ✅ 完成 |
| **Phase 5** | 地形 + 大气 + 水体 + 体积云 + God Ray | ✅ 完成 |
| **Phase 6** | 打磨与工程健康度（测试补全、Shader 错误处理、性能、文档对齐） | 🔲 当前 |

> **备注**：光线追踪（Compute Path Tracer / Hybrid RT / Denoising）已暂缓，待打磨阶段完成后重新规划。

## 📜 许可证

MIT OR Apache-2.0

---

*Aether Engine 是 KongEngine 的精神续作，以 AI-first 架构重新构建。*
