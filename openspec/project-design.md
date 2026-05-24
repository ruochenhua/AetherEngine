# Aether Engine 项目总体设计

## 1. 产品定位

Aether Engine 是一个个人学习和实验性质的现代渲染引擎，
目标是通过 Rust + wgpu 重新实现 KongEngine 的核心渲染能力，
并解决 KongEngine 的架构痛点（双后端维护地狱、RenderModule 上帝类、阴影职责混乱等）。

核心目标：
- **学习渲染原理**：Deferred Shading、PBR、Shadow Mapping、IBL、SSR、SSAO
- **学习光追**：从 Compute Shader Path Tracer 到 Hybrid Ray Tracing
- **实践 ECS 架构**：数据与逻辑解耦，可扩展的系统设计
- **AI 协作友好**：模块化、接口清晰，每个模块适合单次 AI 生成

## 2. 技术演进路线

```
KongEngine (C++/OpenGL/Vulkan)          Aether Engine (Rust/wgpu)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#ifdef RENDER_IN_VULKAN 遍布            ❌ 删除（wgpu 自动适配）
GLSL + SPIR-V 双轨 Shader               ✅ 统一 WGSL
RenderModule 上帝类                     ✅ RenderGraph 驱动
阴影分散在 LightComponent               ✅ 统一 ShadowPass
硬编码 Pass 顺序                        ✅ 声明式依赖排序
OpenGL 隐式状态                         ✅ wgpu 显式 Encoder
无统一 RHI                              ✅ wgpu 即 RHI
物理 TapEngine 未接入                   ✅ ECS 预留 + Phase 4 接入
```

## 3. 核心设计决策

### D1: ECS 库选择 —— hecs

**理由**：
- API 极简，AI 不易写错
- 无宏魔法，代码直观
- 可平滑迁移到 bevy_ecs

### D2: 渲染管线 —— Deferred Shading

**理由**：
- G-Buffer 天然为光追提供表面信息
- 与现代引擎（Unreal/Unity HDRP）对齐
- 光源规模可扩展

### D3: Shader 管理 —— 内联 WGSL

**理由**：
- AI 实现 Pass 时上下文完整
- 无预编译脚本
- 发布时可切 `include_str!()`

### D4: 场景格式 —— RON 为主

**理由**：
- Rust 原生，`serde` 支持完美
- 类型安全，AI 生成不易出错
- 兼容层：YAML 通过转换工具支持

### D5: 物理系统 —— 预留空实现

**理由**：
- Phase 1-3 专注渲染
- ECS Component 预留，编辑器可调参
- Phase 4 接入 rapier3d

## 4. 功能模块

### 4.1 渲染系统 (renderer/)

**Phase 1 核心**：
- G-Buffer Pass（Position/Normal/Albedo/Roughness/Metallic）
- Deferred Lighting Pass（PBR BRDF + 多光源）
- Shadow Pass（CSM + PCF）
- Skybox / IBL Pass
- Post-Process Pass（Tone Mapping ACES）

**Phase 2 扩展**：
- SSAO Pass
- SSR Pass
- Bloom / FXAA
- GPU Instancing

**Phase 3 大场景**：
- Terrain Pass
- Atmosphere Pass
- Water Pass
- Volumetric Cloud Pass

**Phase 4 光追**：
- Compute Path Tracer Pass
- Hybrid RT Reflections Pass
- Denoising Pass

### 4.2 场景系统 (scene/)

- RON 场景描述
- GLTF 模型加载
- YAML 兼容（KongEngine 迁移）
- 运行时实体实例化

### 4.3 资源系统 (asset/)

- Mesh（CPU → GPU）
- Texture（PNG/JPG/HDR）
- Material（PBR Metallic-Roughness）
- Shader（WGSL 编译缓存）

### 4.4 输入系统 (input/)

- 键盘状态跟踪
- 鼠标位置/增量/按钮
- Orbit 相机控制器

### 4.5 UI 系统 (egui)

- 场景对象列表
- 组件属性编辑
- 渲染参数调整
- 性能统计面板

## 5. 数据流设计

### 5.1 一帧的数据流

```
Winit EventLoop
    ↓
InputManager.handle_event()
    ↓
SystemRegistry.run_update(dt)
    ├─ CameraSystem: 更新 Camera / OrbitCamera
    ├─ AnimationSystem: 更新骨骼/材质（预留）
    └─ RenderGraph.build(): 收集 Pass 资源需求
    ↓
RenderContext.begin_frame()
    ↓
RenderGraph.execute(encoder, world)
    ├─ ShadowPass: 绘制阴影贴图
    ├─ GBufferPass: 绘制几何
    ├─ LightingPass: 计算光照
    ├─ SkyboxPass: 绘制天空
    ├─ PostProcessPass: 后处理
    └─ UIPass: 绘制 egui
    ↓
RenderContext.end_frame() (Submit + Present)
```

### 5.2 场景加载数据流

```
SceneLoader.load_from_file("scene/test.ron")
    ↓
ron::de::from_str() → Scene
    ↓
SceneLoader.instantiate(scene, &mut world)
    ├─ Camera Entity: (Transform, Camera)
    ├─ Light Entity: (Transform, Light)
    ├─ Mesh Entity: (Transform, MeshRenderer)
    └─ ...
```

## 6. 性能设计

### 6.1 GPU 侧

- RenderGraph 自动复用 transient texture
- GPU Instancing 批量绘制
- Compute Shader 用于 SSAO / SSR / 光追

### 6.2 CPU 侧

- ECS Query 缓存
- Asset 引用计数（Arc）
- System 并行执行（hecs Query 不冲突时）

### 6.3 内存

- wgpu Buffer/Texture 由设备管理
- AssetManager 缓存已加载资源
- 场景卸载时清理对应 Entity

## 7. 安全与验证

### 7.1 Rust 安全

- 所有权系统防止 use-after-free
- `wgpu` 自动处理 GPU 同步
- `anyhow::Result` 统一错误处理

### 7.2 渲染验证

- `cargo test` 运行单元测试
- 每个 example 验证一组功能
- 视觉回归测试（手动截图对比）

## 8. 扩展性设计

### 8.1 添加新 RenderPass

无需修改现有代码，只需：
1. 实现 `RenderPass` trait
2. `graph.add_pass(MyPass::new())`

### 8.2 添加新 ECS Component

```rust
#[derive(Component)]
pub struct MyComponent { ... }
```

### 8.3 跨平台

wgpu 自动选择后端：
- Windows → Vulkan / DX12
- macOS → Metal
- Linux → Vulkan
- Android → Vulkan
- iOS → Metal

无需 `#ifdef`，一套代码全平台。

---

## 9. Design 阶段工作流规范

> 以下规范基于 `engine-bootstrap` 实践的复盘，确保后续 change 的设计质量。

### 9.1 Dependency Compatibility Matrix（依赖兼容性矩阵）

每个涉及新依赖或版本升级的 change，Design 中必须包含兼容性矩阵：

```markdown
## Dependency Compatibility Matrix

| Crate | 目标版本 | 约束来源 | 兼容性风险 | 验证状态 |
|-------|---------|---------|-----------|---------|
| wgpu  | 0.20    | 直接依赖 | ⚠️ egui-wgpu 0.27 锁定 wgpu 0.19 | 待验证 |
| hecs  | 0.10    | 直接依赖 | ⚠️ `Component` 改为 auto-trait | 待验证 |

验证命令：
```bash
cargo tree -d              # 检查重复版本
cargo check --all-targets  # 全目标编译验证
```
```

**关键原则**：
- 在写第一行代码前完成矩阵验证
- 高风险依赖必须在 Design 阶段解决，不能留到 Apply 阶段
- 版本降级必须在 Design 中说明原因

### 9.2 Impact Analysis（影响面分析）

任何涉及核心 trait、数据模型、公共接口的变更，必须进行影响面分析：

```markdown
## Impact Analysis

- [ ] 新模型 + 兼容层（渐进式迁移策略）
- [ ] 受影响模块清单（使用 grep 统计）

```bash
# 示例：变更 Component trait 的影响面
grep -r "impl Component" crates/aether-engine/src/
grep -r "use.*Component" crates/aether-engine/src/
grep -r "Component" crates/aether-engine/src/ | wc -l
```

- [ ] 渐进式迁移计划
  - Step 1: 新模型 + 兼容层（旧字段保留为别名）
  - Step 2: 按模块渐进迁移
  - Step 3: 删除兼容层
```

**关键原则**：
- 影响面分析必须在 Design 阶段完成，不能留到 Apply 阶段发现
- 全链路影响统计必须精确到每个文件的引用次数
- 渐进式迁移必须分步骤，每步可独立编译通过

### 9.3 接口签名规范

Design 中的接口定义必须标注：
- 生命周期约束（如 `'static`）
- Trait bounds（如 `Send + Sync`）
- 线程安全要求
- 与现有代码的兼容性说明

```rust
// 示例：标注了完整约束的接口
pub async fn new(
    window: Arc<winit::window::Window>  // 需 'static 以满足 Surface<'static>
) -> Self;
```

### 9.4 文档索引

- 详细的工作流模板和检查清单：`openspec/workflow-guides/`
- 项目架构和依赖规则：`openspec/project-architecture.md`
- OpenSpec 全局配置和规则：`openspec/config.yaml`
