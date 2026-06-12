# CODE_MAP — AetherEngine Launcher

AI 开发导航。想改某个功能？从这里找到对应的文件。

## 渲染管线

| 想做什么 | 文件 | 关键位置 |
|----------|------|----------|
| 加新的 Render Pass | `crates/aether-engine/src/renderer/passes/template.rs` | 复制模板 → 填 signature + shader |
| 注册 Pass 到管线 | `crates/aether-launcher/src/pipeline.rs` | `build_pipeline()`: `.add_pass(XxxPass::new())` |
| 改 Pass trait 接口 | `crates/aether-engine/src/renderer/pass.rs` | Pass trait 定义 |
| 改管线构建逻辑 | `crates/aether-engine/src/renderer/pipeline_builder.rs` | PipelineBuilder + 拓扑排序 |
| 改调度执行逻辑 | `crates/aether-engine/src/renderer/scheduler.rs` | Scheduler::execute_all() |

## 编辑器交互

| 想做什么 | 文件 | 关键位置 |
|----------|------|----------|
| 改物体拾取 (Picking) | `crates/aether-engine/src/renderer/picking.rs` | `pick_entity()`: ray vs AABB |
| 改 Gizmo 交互 | `crates/aether-engine/src/renderer/gizmo.rs` | 拖拽 (`apply_drag`) / hover 检测 (`detect_hover`) |
| 改 Hierarchy 面板 | `crates/aether-launcher/src/app.rs` | 搜索 `"hierarchy"` |
| 改 Inspector 面板 | `crates/aether-launcher/src/app.rs` | 搜索 `"Inspector"` 或 `InspectorData` |
| 改 Debug 模式 / 渲染开关 | `crates/aether-launcher/src/app.rs` | 搜索 `"Features"` / `debug_mode` |
| 改场景保存 | `crates/aether-engine/src/scene/serializer.rs` | `serialize_world()` |
| 改场景加载 | `crates/aether-engine/src/scene/loader.rs` | `SceneLoader::open_scene()` |

## 渲染 Pass 索引

| Pass | 文件 | 行数 |
|------|------|------|
| Shadow | `passes/shadow.rs` | ~230 |
| GBuffer | `passes/gbuffer.rs` | ~340 |
| SSAO | `passes/ssao.rs` | ~410 |
| AOBlur | `passes/ao_blur.rs` | ~320 |
| Lighting | `passes/lighting.rs` | ~840 |
| SSR | `passes/ssr.rs` | ~710 |
| Composite | `passes/composite.rs` | ~130 |
| Bloom | `passes/bloom.rs` | ~1020 |
| ToneMapping | `passes/tone_mapping.rs` | ~390 |
| FXAA | `passes/fxaa.rs` | ~510 |
| DebugLine | `passes/debug.rs` | ~390 |

## 资源标签

| 标签 | 含义 | 定义位置 |
|------|------|----------|
| `GPosition` | GBuffer 位置 | `resource.rs` |
| `GNormal` | GBuffer 法线 | `resource.rs` |
| `GAlbedo` | GBuffer 反照率 | `resource.rs` |
| `GMaterial` | GBuffer 材质参数 | `resource.rs` |
| `AOTexture` | SSAO 输出 | `resource.rs` |
| `AOTextureBlurred` | SSAO 模糊输出 | `resource.rs` |
| `Swapchain` | 最终输出 | `resource.rs` |

## 已知陷阱速查

见 `CONTEXT.md` § 已知陷阱（跨模块）。模块内部实现细节见各文件头部的 doc comment。
