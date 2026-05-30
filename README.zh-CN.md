# Aether Engine

[English](README.md) | [简体中文](README.zh-CN.md)

一个基于 **Rust** 和 **wgpu** 构建的现代渲染引擎，用于学习从 Deferred PBR 到实时光线追踪的实时图形技术。

## 🌟 特性

- **现代架构**：ECS (hecs) + RenderGraph 驱动管线
- **跨平台**：wgpu 自动适配 Vulkan/Metal/DX12
- **延迟着色**：基于 G-Buffer 的 Blinn-Phong PBR，支持分通道调试
- **UE 风格飞行相机**：右键漫游，WASD + QE 移动，滚轮调速
- **调试工具**：世界网格、RGB 三轴指示器、光照分通道可视化
- **可扩展**：添加新 RenderPass 无需修改现有代码
- **AI 友好**：模块化设计，每个模块适合单次 AI 生成

## 🚀 快速开始

```bash
# 克隆仓库
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine

# 构建
cargo build

# 启动 Launcher（推荐入口）
cargo run -p aether-launcher

# 或单独运行示例
cargo run --example 01_triangle
cargo run --example 02_deferred
cargo run --example 03_gltf_scene
```

## 🎮 操控（02_deferred）

| 按键 | 功能 |
|------|------|
| `鼠标右键` | 切换飞行模式 |
| `W A S D` | 前 / 左 / 后 / 右 |
| `Q` / `E` | 下降 / 上升（世界空间） |
| `鼠标` | 旋转视角（飞行模式） |
| `滚轮` | 调节移动速度 |
| `0` – `5` | 光照调试：完整 / 环境光 / 漫反射 / 高光 / 法线 / NdotL |
| `Esc` | 返回 Launcher 菜单 |

## 📁 项目结构

```
├── Cargo.toml
├── crates/
│   ├── aether-engine/          # 主引擎 crate
│   │   └── src/
│   │       ├── lib.rs            # 公开 API
│   │       ├── app.rs            # 独立应用入口
│   │       ├── ecs/              # ECS (hecs 封装)
│   │       ├── scene/            # 场景加载/序列化
│   │       ├── asset/            # 资源管理
│   │       ├── renderer/         # 渲染核心
│   │       │   ├── graph.rs      # RenderGraph
│   │       │   ├── context.rs    # wgpu 上下文 + GBuffer
│   │       │   ├── camera.rs     # FlyCamera + OrbitCamera
│   │       │   └── passes/
│   │       │       ├── gbuffer.rs   # G-Buffer (MRT)
│   │       │       ├── lighting.rs  # 延迟光照
│   │       │       └── debug.rs     # 线段渲染（网格、坐标轴）
│   │       ├── physics/          # 物理系统（预留）
│   │       ├── math.rs           # 数学工具
│   │       ├── input.rs          # 输入管理
│   │       └── examples/         # Example 实现
│   └── aether-launcher/         # 统一 Launcher 程序
├── assets/
│   ├── scenes/                   # .ron 场景文件
│   ├── shaders/                  # .wgsl 着色器
│   ├── meshes/                   # GLTF 模型
│   └── textures/                 # 贴图
└── docs/
    └── adr/                      # 架构决策记录
```

## 🏗️ 架构

### 渲染管线

```
Launcher (winit 事件循环)
  └── Example (trait)
        ├── update(dt, input)     # 相机、输入、逻辑
        ├── prepare()             # GPU 数据上传
        └── render(encoder)       # 命令录制
              ├── GBufferPass     # → 位置、法线、颜色、材质
              ├── LightingPass    # → 全屏四边形、Blinn-Phong
              └── DebugLinePass   # → 网格、坐标轴（带深度测试）
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
| **Phase 0** | 窗口、三角形、egui、Launcher | ✅ 完成 |
| **Phase 1** | Deferred PBR、飞行相机、调试工具 | 🚧 进行中 |
| **Phase 2** | 阴影、IBL、场景 YAML | 🔲 计划中 |
| **Phase 3** | SSR + SSAO + 后处理 | 🔲 计划中 |
| **Phase 4** | 地形 + 大气 + 水体 + 体积云 | 🔲 计划中 |
| **Phase 5** | 光线追踪（Compute + Hybrid） | 🔲 计划中 |

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
