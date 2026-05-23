# Aether Engine 项目架构文档

## 1. 技术架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                        应用层 (App / Examples)                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  示例程序     │  │   编辑器      │  │    调试工具          │  │
│  │  (examples/) │  │  (egui)      │  │   (性能面板)         │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
└─────────┼─────────────────┼─────────────────────┼──────────────┘
          │                 │                     │
          ▼                 ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│                     引擎核心层 (Engine Core)                     │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    ECS World (hecs)                        │  │
│  │  Entities ──▶ Components (Transform, Mesh, Light, ...)    │  │
│  │  Systems ──▶ Read/Write Query                             │  │
│  └───────────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              System Registry (调度中心)                     │  │
│  │  Startup: Init systems                                     │  │
│  │  FixedUpdate (60Hz): Physics                               │  │
│  │  Update: Camera, Animation, Culling, RenderGraph.Build     │  │
│  │  Render: RenderGraph.Execute(encoder, world)               │  │
│  │  Shutdown: Cleanup                                         │  │
│  └───────────────────────────────────────────────────────────┘  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  RenderGraph │  │  AssetMgr   │  │   Input/Window          │  │
│  │  (Pass调度)  │  │  (资源管理)  │  │   (winit + wgpu surface)│  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                     系统实现层 (Systems)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Renderer    │  │  Physics    │  │    Audio (预留)         │  │
│  │ (wgpu)      │  │  (预留)      │  │                         │  │
│  │ ├─Pass      │  │             │  │                         │  │
│  │ ├─Shader    │  │             │  │                         │  │
│  │ ├─Material  │  │             │  │                         │  │
│  │ └─Mesh      │  │             │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## 2. 模块依赖关系

```
app/ ──▶ ecs/
   │        │
   │        ▼
   │    scene/ ◀── asset/
   │        │
   ▼        ▼
renderer/ ◀── math/ ◀── input/ ◀── window/
   │
   ▼
physics/ (预留，未来通过 ECS 解耦)
```

**依赖规则**：
- `app` 可以依赖所有模块
- `renderer` 只依赖 `ecs`, `math`, `asset`
- `scene` 依赖 `ecs`, `asset`
- `physics` 只依赖 `ecs`, `math`（预留阶段不依赖 renderer）
- 不允许循环依赖

## 3. 核心模块详解

### 3.1 ECS 系统 (ecs/)

**位置**：`crates/aether-engine/src/ecs/`

**职责**：
- `World`：hecs World 的薄封装
- `System` trait：定义系统生命周期
- `SystemRegistry`：系统注册和调度

**接口**：
```rust
pub trait System {
    fn name(&self) -> &str;
    fn init(&mut self, world: &mut World);
    fn update(&mut self, dt: f32, world: &mut World);
    fn shutdown(&mut self, world: &mut World);
}
```

### 3.2 RenderGraph (renderer/graph.rs)

**职责**：
- 声明式 Pass 依赖管理
- 自动拓扑排序
- Transient 资源分配

**接口**：
```rust
pub trait RenderPass {
    fn name(&self) -> &str;
    fn declare_resources(&self, builder: &mut PassResourceBuilder);
    fn execute(&self, encoder: &mut CommandEncoder, context: &RenderContext, world: &World);
}
```

### 3.3 AssetManager (asset/mod.rs)

**职责**：
- 资源加载和缓存
- 路径到 Handle 的映射
- 引用计数（Arc）

**接口**：
```rust
pub struct AssetManager {
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> Result<Handle<T>>;
    pub fn get<T: 'static>(&self, handle: Handle<T>) -> Option<Arc<T>>;
}
```

### 3.4 Scene 系统 (scene/)

**职责**：
- 场景序列化（RON/YAML）
- 实体实例化到 ECS World
- 与 KongEngine 场景格式兼容（远期）

## 4. 渲染管线架构

### 4.1 Pass 顺序（Deferred Shading）

```
Shadow Pass          → 阴影贴图
    ↓
G-Buffer Pass        → Position/Normal/Albedo/Rough/Metal
    ↓
SSAO Pass            → 环境光遮蔽（可选）
    ↓
Lighting Pass        → PBR Deferred Lighting
    ↓
Skybox Pass          → 天空盒 / IBL
    ↓
SSR Pass             → 屏幕空间反射（可选）
    ↓
Post-Process Pass    → Tone Mapping + FXAA + Bloom
    ↓
UI Pass              → egui
```

### 4.2 资源流转

```
Shadow Pass          输出: shadow_map
G-Buffer Pass        输入: scene_meshes
                     输出: gbuffer_position, gbuffer_normal, gbuffer_albedo, gbuffer_material
SSAO Pass            输入: gbuffer_position, gbuffer_normal
                     输出: ssao_texture
Lighting Pass        输入: gbuffer_*, shadow_map, ssao_texture, lights
                     输出: lit_scene_color
Skybox Pass          输入: lit_scene_color, gbuffer_depth
                     输出: scene_color_with_sky
SSR Pass             输入: scene_color_with_sky, gbuffer_*
                     输出: scene_color_with_reflections
Post-Process Pass    输入: scene_color_*
                     输出: final_color
UI Pass              输入: final_color
                     输出: swapchain
```

## 5. 扩展点

### 5.1 添加新 RenderPass

1. 在 `renderer/passes/` 创建新文件
2. 实现 `RenderPass` trait
3. 在 `Renderer::new()` 中注册到 RenderGraph

### 5.2 添加新 ECS System

1. 创建 System struct
2. 实现 `System` trait
3. 在 `SystemRegistry` 中注册

### 5.3 添加新 Asset 类型

1. 实现 `Asset` trait
2. 在 `AssetManager` 中使用

## 6. 物理预留 (physics/)

**当前状态**：Phase 1-3 为空实现

**预留组件**：
- `RigidBody`：速度、质量、静态标记
- `Collider`：形状、摩擦、弹性

**未来接入**：
- Phase 4 实现 `physics_system`，可接入 rapier3d

## 7. 构建与运行

```bash
# 构建
cargo build

# 运行示例（未来）
cargo run --example 01_triangle

# 测试
cargo test

# 检查
cargo clippy
```
