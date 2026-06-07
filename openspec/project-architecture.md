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
Composite Pass       → 场景 + SSR 混合
    ↓
DebugLine Pass       → Gizmo / Grid 线框覆盖
    ↓
Post-Process Pass    → Tone Mapping + FXAA + Bloom（预留）
    ↓
UI Pass              → egui（Editor 面板 + Inspector）
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
                     输出: reflection_texture
Composite Pass       输入: lit_scene_color, reflection_texture
                     输出: composite_color
DebugLine Pass       输入: composite_color
                     输出: final_color (with debug lines overlaid)
Post-Process Pass    输入: final_color
                     输出: post_processed_color（预留）
UI Pass              输入: final_color
                     输出: swapchain
```

## 5. 扩展点

### 5.1 添加新 Pass

1. 在 `renderer/passes/` 创建新文件
2. 实现 `Pass` trait（`init` → `resolve` → `execute`）
3. 在 `Launcher` 中创建并注册到 `PipelineBuilder`
4. 声明 `PassSignature` 定义资源读写依赖

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

# 运行示例
cargo run --example 01_triangle

# 测试
cargo test

# 检查
cargo clippy
```

---

## 8. 依赖管理与版本策略

### 8.1 版本冲突预防

**engine-bootstrap 的教训**：wgpu 0.20 与 egui-wgpu 0.27 的版本冲突消耗了约 30% 的编码时间。

**预防机制**：
1. **Design 阶段强制验证**：每个 change 的 Design 必须包含 Dependency Compatibility Matrix
2. **添加新依赖前检查**：
   ```bash
   cargo tree -d              # 检查重复版本
   cargo tree -i wgpu         # 查看谁依赖了 wgpu
   cargo update --dry-run     # 预览版本变更
   ```
3. **版本锁定策略**：
   - 优先使用 workspace 级别的版本统一
   - 避免在同一 workspace 中引入同一 crate 的多个 major 版本
   - 如果必须多版本共存，在 Design 中说明隔离方案

### 8.2 依赖升级流程

```
1. 检查 changelog / migration guide
2. cargo update --dry-run 预览
3. 修改 Cargo.toml 版本号
4. cargo check 验证编译
5. 修复 API 变更（生命周期、trait bounds 等）
6. cargo test 验证功能
7. 更新文档中的接口签名
```

### 8.3 已知的版本约束（基线）

| Crate | 当前版本 | 锁定原因 | 升级路径 |
|-------|---------|---------|---------|
| wgpu  | 29.0    | egui-wgpu 0.34 同步 | 跟随 wgpu/egui 生态统一升级 |
| egui  | 0.34    | egui-wgpu/egui-winit 同步 | 跟随 egui 生态统一升级 |
| hecs  | 0.11    | API 稳定 | 可独立升级 |
| winit | 0.30    | egui-winit 0.34 同步 | 跟随 egui 生态统一升级 |
| glam  | 0.33    | bytemuck 特性用于 GPU 上传 | 可独立升级 |

## 9. 影响面分析工具箱

### 9.1 核心变更的影响面 grep 模板

**Trait 变更**（如 `Component`、`RenderPass`）：
```bash
grep -rn "impl <TRAIT>" crates/aether-engine/src/
grep -rn "use .*<TRAIT>" crates/aether-engine/src/
grep -rn "dyn <TRAIT>" crates/aether-engine/src/
grep -rn "<TRAIT>" crates/aether-engine/src/ | wc -l
```

**字段/类型变更**：
```bash
grep -rn "old_field_name" crates/aether-engine/src/
grep -rn "OldTypeName" crates/aether-engine/src/ | wc -l
```

**公共 API 变更**：
```bash
grep -rn "pub fn old_name" crates/aether-engine/src/
grep -rn "pub struct OldName" crates/aether-engine/src/
```

### 9.2 全链路影响统计模板

```markdown
## 影响面统计

| 文件路径 | 引用次数 | 影响类型 | 迁移策略 |
|---------|---------|---------|---------|
| src/ecs/mod.rs | 3 | trait 定义变更 | 直接修改 |
| src/scene/mod.rs | 5 | 导入变更 | 直接修改 |
| src/renderer/mesh.rs | 2 | impl 块移除 | 直接修改 |
| **总计** | **10** | — | — |
```

## 10. 编译-修复循环的成本预估

基于 engine-bootstrap 的经验数据：

| 变更类型 | 预估编码时间 | 预估编译修复时间 | 总时间 |
|---------|------------|----------------|-------|
| 新增独立模块 | 2h | 0.5h | 2.5h |
| 接口签名调整 | 1h | 1h | 2h |
| 依赖版本升级 | 0.5h | 1-3h | 1.5-3.5h |
| ECS trait 迁移 | 2h | 2h | 4h |
| 跨模块重构 | 3h | 3h | 6h |

**任务拆分原则**：
- 任何涉及 trait / 生命周期 / 版本升级的 change，预留 **50% buffer** 给编译修复
- Task 中必须将「编译通过」和「运行时验证」作为独立任务
- 如果编译错误超过 20 个，暂停编码，回退到 Design 阶段重新评估

---

## 11. 文档索引

- 设计阶段检查清单：`openspec/workflow-guides/design-checklist.md`
- Task 拆分标准模板：`openspec/workflow-guides/task-template.md`
- 项目设计规范（Dependency Matrix + Impact Analysis）：`openspec/project-design.md`
- OpenSpec 全局配置和规则：`openspec/config.yaml`
