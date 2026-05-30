# 统一基础设施 + 场景描述方案讨论

> 状态：待决策 | 2026-05-30

## 背景

当前 AetherEngine 的 Example 系统存在两个问题：

1. **基础设施重复**：`FlyCamera`、debug 网格/Gizmo、输入处理嵌入在 `deferred.rs` 中，其他 example（`gltf_scene.rs`、`triangle`）需要各自实现或无法使用
2. **缺少场景描述格式**：每个 example 在代码中硬编码 renderables，添加新场景需写 Rust 代码

参考 KongEngine 的做法：输入控制、基础渲染设施为一套共享层，不同场景用 YAML 文件描述和管理。

## 当前状态

| 组件 | deferred.rs | gltf_scene.rs | triangle |
|---|---|---|---|
| 相机 | FlyCamera（自管） | OrbitCamera（自管） | 无 |
| 输入 | WASD/QE/滚轮 | WASD/mouse_delta | 无 |
| 调试渲染 | 网格 + Gizmo | 无 | 无 |
| 场景定义 | 硬编码 Renderable | 硬编码 Renderable | 硬编码 |

## 目标

1. 所有 example 共享同一套输入/相机/调试基础设施
2. 场景通过声明式文件（YAML）描述，无需写代码即可添加
3. 保留自定义逻辑的扩展能力

## 架构选项

### 方案 A：精简 Example trait

保留 `Example` trait，但将通用基础设施提升到 Launcher 层。

```
Launcher（持有）
  ├── FlyCamera          ← 始终激活，所有 example 共享
  ├── DebugLinePass      ← 始终渲染 grid + gizmo
  ├── InputManager       ← 统一分发
  └── Example trait（精简）
        ├── scene(&self) → Vec<Renderable>  ← 场景数据
        ├── update(&mut self, dt, input)    ← 可选的自定义逻辑
        └── ui(&mut self, egui_ctx)         ← 可选的自定义 UI
```

| 优点 | 缺点 |
|---|---|
| 改动最小，渐进迁移 | 仍有 trait 样板代码 |
| 现有 example 可分步适配 | 添加新场景仍需写 Rust struct |
| 保留自定义逻辑能力 | |

### 方案 B：纯 YAML 场景

废除 `Example` trait，Launcher 直接加载 YAML 场景文件。

```yaml
# scenes/02_deferred.yaml
name: "Deferred Shading Demo"
camera:
  type: fly
  position: [3.0, 3.0, 3.0]
  yaw: -2.356
  pitch: -0.785
lights:
  - type: directional
    direction: [0.0, -1.0, 0.0]
    color: [1.0, 1.0, 1.0]
    intensity: 1.0
ambient: 0.05
objects:
  - mesh: cube
    position: [-0.8, 0.0, 0.0]
    material:
      albedo: [0.8, 0.3, 0.2, 1.0]
      roughness: 0.5
      metallic: 0.0
  - mesh: sphere
    position: [0.8, 0.0, 0.0]
    material:
      albedo: [0.2, 0.5, 0.8, 1.0]
      roughness: 0.05
      metallic: 0.0
```

Launcher 启动→扫描 YAML 目录→构建菜单→加载选中的场景。

| 优点 | 缺点 |
|---|---|
| 零代码添加新场景 | 无法表达复杂逻辑（动画、交互） |
| 完全声明式，AI 友好 | YAML 解析 + 类型校验需要额外工作 |
| 与 KongEngine 思路一致 | 现有 example 需要重新表达为 YAML |

### 方案 C：YAML + 可选脚本（混合）

YAML 描述静态场景，`Example` trait 保留给需要自定义逻辑的场景。

```
Launcher
  ├── 内置场景（从 YAML 加载）
  │     ├── scenes/basic.yaml
  │     └── scenes/multi_object.yaml
  └── 动态场景（实现 Example trait）
        ├── deferred.rs
        └── gltf_scene.rs
```

内置场景使用 YAML 描述，动态场景实现 trait。两者在 Launcher 菜单中混合展示。

| 优点 | 缺点 |
|---|---|
| 两全其美 | 两套机制增加复杂度 |
| 简单场景零代码，复杂场景全能力 | YAML 场景和 Example 的 UI/update 接口需要统一 |
| 渐进式迁移路径 | |

## 决策依赖

在选择方案前，需先明确以下问题：

1. **是否需要自定义逻辑？** 如果所有场景都是静态物体展示，方案 B 足够。如果需要动画、脚本、交互式编辑，方案 A 或 C。
2. **YAML vs RON？** 当前 CONTEXT.md 中场景格式为 RON（Rust 原生类型安全），YAML 更通用。是否需要切换？
3. **Launcher 的职责边界？** Launcher 持有 camera + debug，是否也持有渲染管线（GBufferPass、LightingPass）？Currently 这些在 Example 内部。
4. **内置 mesh 命名？** YAML 中需要引用 `cube`、`sphere`、`quad` 等——需要建立命名→CpuMesh 的映射表。

## 后续

- 确定方案后创建 ADR
- Phase 1 路线图中 "场景 YAML" 提前到当前阶段
