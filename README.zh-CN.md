# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

一个基于 **Rust** 和 **wgpu** 构建的现代渲染引擎，用于学习从 Deferred PBR 到实时光线追踪的实时图形技术。

## 🌟 特性

- **现代架构**：ECS (hecs) + RenderGraph 驱动管线
- **跨平台**：wgpu 自动适配 Vulkan/Metal/DX12
- **延迟着色**：基于 G-Buffer 的 PBR 渲染
- **可扩展**：添加新 RenderPass 无需修改现有代码
- **AI 友好**：模块化设计，每个模块适合单次 AI 生成

## 🚀 快速开始

```bash
# 克隆仓库
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# 构建
cargo build

# 运行示例（待实现）
cargo run --example 01_triangle
```

## 📁 项目结构

```
├── Cargo.toml
├── crates/
│   └── aether-engine/          # 主引擎 crate
│       └── src/
│           ├── lib.rs            # 公开 API
│           ├── app.rs            # 应用入口
│           ├── ecs/              # ECS (hecs 封装)
│           ├── scene/            # 场景加载/序列化
│           ├── asset/            # 资源管理
│           ├── renderer/         # 渲染核心
│           │   ├── graph.rs      # RenderGraph
│           │   ├── context.rs    # wgpu 上下文
│           │   └── passes/       # 渲染 Pass
│           ├── physics/          # 物理系统（预留）
│           ├── math.rs           # 数学工具
│           ├── input.rs          # 输入管理
│           └── window.rs         # 窗口封装
├── assets/
│   ├── scenes/                   # .ron 场景文件
│   ├── shaders/                  # .wgsl 着色器
│   ├── meshes/                   # GLTF 模型
│   └── textures/                 # 贴图
├── examples/                     # 示例程序
└── openspec/                     # OpenSpec 工作流
```

## 🏗️ 架构

### ECS + RenderGraph

```
App (winit 事件循环)
  └── SystemRegistry (系统注册中心)
        ├── Update: Camera, Animation
        └── Render: RenderGraph
              ├── ShadowPass          # 阴影
              ├── GBufferPass         # 几何
              ├── LightingPass        # 光照
              ├── SkyboxPass          # 天空盒
              ├── PostProcessPass     # 后处理
              └── UIPass              # UI
```

### 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| ECS 库 | `hecs` | API 极简，AI 友好，无宏魔法 |
| 渲染 API | `wgpu` | 单一后端，自动适配 Vulkan/Metal/DX12 |
| 着色器语言 | WGSL | 统一，无需预编译脚本 |
| 场景格式 | RON | Rust 原生，类型安全，AI 友好 |
| UI | `egui` | 即时模式，易于调试面板 |

## 📅 路线图

| 阶段 | 特性 | 状态 |
|------|------|------|
| **Phase 0** | 骨架（窗口、三角形、egui） | 🚧 进行中 |
| **Phase 1** | Deferred PBR + 阴影 + IBL | 🔲 计划中 |
| **Phase 2** | SSR + SSAO + 后处理 | 🔲 计划中 |
| **Phase 3** | 地形 + 大气 + 水体 + 体积云 | 🔲 计划中 |
| **Phase 4** | 光线追踪（Compute + Hybrid） | 🔲 计划中 |

## 🤝 参与贡献

这是一个个人学习项目。代码库设计为 AI 协作友好：

- 每个模块自包含（< 500 行）
- 清晰的 trait 接口（`RenderPass`, `System`, `Asset`）
- 无复杂泛型约束
- 全面的文档注释

## 📜 许可证

MIT OR Apache-2.0

---

*Aether Engine 是 [KongEngine](https://github.com/ruochenhua/KongEngine) 的精神续作，用现代架构和 Rust 重新构建。*
