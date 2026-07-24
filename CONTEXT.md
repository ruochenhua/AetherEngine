# AetherEngine — 领域上下文

## 项目定位

Rust + wgpu 现代渲染引擎，KongEngine 的精神续作。目标：从 Deferred PBR 学到实时光追。

## 核心领域语言

| 术语 | 含义 |
|------|------|
| **Launcher** | 引擎的统一入口程序。持有唯一的 Window、RenderContext、渲染管线、FlyCamera、DebugGrid/Gizmo。扫描 `scenes/` 目录发现 RON 场景文件，提供菜单切换场景 |
| **Scene** | 场景：通过 RON 文件描述的静态 3D 场景。包含相机初始参数、灯光配置、物体列表（mesh + transform + material）。Launcher 加载后 spawn 为 ECS World 中的 entity，经 Extract 阶段输出 RenderBatch 供 GPU 渲染 |
| **SceneLoader** | 从 RON 文件读取 → spawn ECS entity（Transform + MeshHandle + MaterialUniform）+ 构建 LightingUniforms 的转换器 |
| **BuiltinMeshRegistry** | 内置基础网格的名称→CpuMesh 映射表（cube / sphere / quad / plane） |
| **PipelineBuilder** | 管线构建器。收集各 Pass 的 `PassSignature`，`build()` 完成拓扑排序、纹理分配、Pass resolve。见 `crates/aether-engine/src/renderer/pipeline_builder.rs` |
| **Scheduler** | 执行调度器。`execute_all()` 按拓扑序调用 Pass，`rebuild()` 重建分辨率相关纹理 |
| **Pass** | 渲染阶段统一 trait。三阶段：`init()` → `resolve(resource_table)` → `execute(encoder, resource_table)`。新 Pass 模板：`passes/template.rs` |
| **PassSignature** | Pass 资源依赖声明：列出 reads/writes（`ResSlot<TypeTag>`） |
| **ResourceTable** | 构建期资源映射表，`name → view`。各 Pass 通过句柄获取纹理 |
| **ResHandle<T>** | 类型安全资源句柄。T 为零大小标签（`GPosition`、`AOTexture`、`Swapchain`），编译期防止纹理语义混用 |
| **G-Buffer** | 延迟渲染几何缓冲（Position/Normal/Albedo/Material） |
| **Extract** | 渲染提取阶段。每帧将 ECS World 中渲染相关 Component 提取为 GPU-ready 的 `Vec<RenderObject>` |
| **RenderObject** | 提取阶段的输出单元。包含 model_matrix、mesh_handle、material_uniform |
| **RenderBatch** | 按 `(mesh_handle, material_key)` 分组，供 instancing 使用 |
| **ECS** | hecs 实体组件系统。Phase 3 起成为场景数据和编辑器状态的唯一真相源 |
| **RON** | Rusty Object Notation，场景描述格式 |
| **MRT** | Multiple Render Targets，GBufferPass 同时写入 4 个纹理 |
| **IBL** | Image-Based Lighting，基于图像光照 |
| **FlyCam** | UE 风格自由飞行相机：`Alt+左键`环顾，WASD 移动，滚轮调速 |
| **Picking** | CPU 端通过 camera ray 与 ECS World 中物体的 AABB 做相交检测，返回选中的 Entity |
| **Gizmo** | 变换操作器（平移/旋转/缩放），通过 DebugLinePass 渲染，左键拖拽直接修改 Transform |
| **Terrain** | 地形渲染系统。A 级 = 单高度图 displacement；B 级 = 可漫游地形 + LOD + 视锥裁剪；C 级 = 大世界观 + tile streaming + 虚拟纹理。Phase 5 目标为 B 级 |
| **TerrainSource** | 地形高度数据来源抽象。至少支持 `Heightmap(path)` 和 `Procedural(seed, params)` 两种模式，未来可扩展为 `RuntimeGenerated` |
| **TerrainGeometry** | 地形几何生成策略。Phase 5 使用 Chunked LOD；长期目标为 Compute Tessellation + Indirect Draw |
| **TerrainPass** | 可选的渲染 Pass。当 Scene 配置了 Terrain 时注册到管线，输出 GBuffer 数据供 LightingPass 复用 |
| **Pipeline Rebuild** | Scheduler 的重建机制。窗口大小变化、场景切换、或启用/禁用可选 Pass（如 TerrainPass）时触发 |
| **Terrain Entity** | Scene 中代表地形整体的一个 ECS entity，携带 `Terrain` component。Editor Hierarchy 中显示为单一可折叠节点；具体的 chunks 由 TerrainPass 内部管理 |
| **Atmosphere** | 大气渲染系统。Phase 5 使用解析模型（Preetham/Hosek-Wilkie）替代静态 skybox 背景，并输出 sun/sky 颜色供光照使用；IBL 仍复用配置的 HDR cubemap。长期目标为物理大气散射（Bruneton/Nishita LUT） |
| **WaterPass** | 透明 Forward Pass。在 Deferred 不透明管线之后渲染水面，支持 Gerstner 波浪、SSR 反射、屏幕空间折射 |
| **CloudPass** | 独立体积云 Pass。使用预生成 3D noise texture + 全分辨率 ray marching，写入独立的 `CloudColor`（Rgba16Float）纹理，由 CompositePass 读取混合；`should_run` 仅在场景含云时执行；Phase 5 仅做装饰性天空云，不投射阴影 |
| **Worktree** | git worktree 机制：一个 issue = 一个 worktree。每个 worktree 有独立的 `.aether-changes.yml` |
| **aether-merge** | 基于 change manifest 的结构化合并编排 skill。见 `.claude/skills/aether-merge/SKILL.md` |
| **冲突等级** | L0 无重叠 → L1 同文件不同函数 → L2 签名变更 → L3 同函数体冲突 |

## 架构约束

- 一个文件一个职责（single responsibility）；纯逻辑代码（不含内联 shader）约 600 行为健康区，超过约 800 行且能识别出第二个独立职责时才拆分——拆分依据是职责而非行数，内联 shader 不计入行数
- 无复杂泛型约束，Trait 接口清晰
- wgpu 自动适配后端（Vulkan/Metal/DX12）
- 加 Pass：复制 `passes/template.rs`，填 signature + shader，在 `build_pipeline()` 中注册一行

## 技术演进路线

```
Phase 0: Skeleton (window, triangle, egui)                         ← ✅ 已完成
Phase 1: Deferred PBR + Shadow + Pipeline + RON + IBL              ← ✅ 已完成
Phase 2: SSAO + SSR                                                 ← ✅ 已完成
Phase 3: Editor (ECS runtime, Picking, Gizmo, UI, Save/Load)       ← ✅ 已完成
Phase 4: Post-process chain + Tone mapping + Bloom + FXAA          ← ✅ 已完成
Phase 5: 大场景渲染 (Terrain + Atmosphere + Water + Clouds + God Rays) ← ✅ 已完成
Phase 6: 打磨与工程健康度 (测试补全 + 性能 + 文档对齐)              ← 🔲 当前
         （光线追踪已暂缓，待打磨完成后重新评估）
```

## 已知陷阱（跨模块）

以下陷阱影响多个模块或涉及全局约定，AI 修改任何渲染代码时应特别注意：

- **wgpu MRT**: 不支持多 fragment entry points，必须用单 `fs_main` 返回 `FragmentOutput` struct
- **Normal 编码**: GBuffer `*0.5+0.5`，Lighting `*2.0-1.0`
- **Surface `'static`**: `Arc<Window>` 需满足生命周期
- **全屏四边形 UV 翻转**: wgpu NDC Y=1 是顶，正确公式：`uv = vec2(x*0.5+0.5, 0.5 - y*0.5)`
- **FlyCam right() 叉积顺序**: `forward × world_up`（正确）vs `world_up × forward`（错误），影响 A/D 方向
- **Shadow map Z 映射**: `orthographic_rh` 输出 z∈[-1,1]，但 wgpu depth buffer 用 z∈[0,1]。需自写兼容正交矩阵
- **Shadow UV Y-flip**: NDC y=-1 对应纹理 y=0，需 `uv.y = 0.5 - ndc.y*0.5`
- **Shadow bias**: 软件 slope-scale bias，公式 `tan(acos(NdotL))`，base=0.005 NDC。见 ADR-0004
- **IBL cubemap 投影**: `capture_projection()` 必须用 `correction * p_gl`，x/y 同步缩放 2x。见 ADR-0006
- **HDR 加载翻转**: `image` crate HDR 原点左上角，equirect 原点左下角。±Y 面对调补偿
- **Pass 执行顺序与 DebugLine 覆盖**: DebugLinePass 必须最后执行——它最后注册且是 Swapchain 的最后顺序写入者，拓扑排序自然将其排到末尾
- **egui 事件消费与 picking**: 所有指针事件（CursorMoved/MouseInput/MouseWheel）都更新 `egui_consumed_pointer`，防止 stale true 持续阻止 picking
- **体积云球壳中心放置约定**: 云球壳中心必须放在相机正下方的地轴上，即 `(cam_pos.x, -planet_radius, cam_pos.z)`，而不是相机位置本身；否则相机抬升/下降到任意高度时云壳渲染错误。见 `crates/aether-engine/src/renderer/passes/volumetric_cloud/mod.rs`（`apply_frame` 内行内注释），对应修复 commit e7371a0

其他实现细节（如球体绕序、per-object draw 顺序、hierarchy 排序）已迁至对应模块的 doc comment 中。

## 关键文件

| 文件 | 职责 |
|------|------|
| `crates/aether-engine/src/renderer/pipeline_builder.rs` | PipelineBuilder + 拓扑排序 + 纹理分配 |
| `crates/aether-engine/src/renderer/scheduler.rs` | Scheduler 执行调度 + 运行时 pass 参数设置 |
| `crates/aether-engine/src/renderer/passes/` | 所有 Render Pass 实现 |
| `crates/aether-launcher/src/main.rs` | 入口（~5 行） |
| `crates/aether-launcher/src/app.rs` | Launcher App：事件循环 + UI + 编辑器交互 |
| `crates/aether-launcher/src/pipeline.rs` | 管线构建 helper + 截图工具 |
| `scenes/` | RON 场景文件目录 |
| `docs/adr/` | 架构决策记录 |

## 决策记录

见 `docs/adr/` 目录。
