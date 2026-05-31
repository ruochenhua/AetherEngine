# AetherEngine — 领域上下文

## 项目定位

Rust + wgpu 现代渲染引擎，KongEngine 的精神续作。目标：从 Deferred PBR 学到实时光追。

## 核心领域语言

| 术语 | 含义 |
|------|------|
| **Launcher** | 引擎的统一入口程序。持有唯一的 Window、RenderContext、渲染管线、FlyCamera、DebugGrid/Gizmo。扫描 `scenes/` 目录发现 RON 场景文件，提供菜单切换场景 |
| **Scene** | 场景：通过 RON 文件描述的静态 3D 场景。包含相机初始参数、灯光配置、物体列表（mesh + transform + material）。Launcher 加载后展开为 GPU Renderable 列表 |
| **SceneDescription** | 场景 RON 文件反序列化后的内存数据结构。包含 `name`、`camera`、`lights`、`objects` |
| **SceneLoader** | 从 RON 文件读取 → `SceneDescription` → 构建 GPU 资源（`Vec<Renderable>` + `LightingUniforms`）的转换器 |
| **BuiltinMeshRegistry** | 内置基础网格的名称→CpuMesh 映射表（cube / sphere / quad / plane）。RON 对象可通过 "#cube" 前缀引用内置网格，也可通过文件路径加载外部网格 |
| **RenderGraph** | （已废弃）声明式渲染管线编排，Pass 依赖自动排序。由 PipelineBuilder / Scheduler 取代 |
| **PipelineBuilder** | 管线构建器。收集各 Pass 的 `PassSignature` 声明，`build()` 时完成拓扑排序、transient 纹理分配、`ResHandle<T>` 注入 |
| **Scheduler** | 执行调度器。`execute()` 按拓扑序调用 Pass，`rebuild()` 重建分辨率相关纹理 |
| **Pass** | 渲染阶段统一 trait。三阶段生命周期：`init()` → `resolve(resource_table)` → `execute(encoder, resource_table)` |
| **PassSignature** | Pass 资源依赖声明：列出 reads 和 writes（`ResSlot<TypeTag>`），格式：`(type_tag, name)` |
| **ResourceTable** | 构建期资源映射表，`name → ResHandle<T>`。各 Pass 通过句柄获取纹理 View |
| **ResHandle<T>** | 类型安全资源句柄。T 为零大小标签（`GPosition`、`AOTexture`、`Swapchain`），编译期防止纹理语义混用 |
| **G-Buffer** | 延迟渲染几何缓冲（Position/Normal/Albedo/Material） |
| **RenderPass** | （已废弃）原 Pass trait。由新的 `Pass` trait 取代（见 ADR-0003） |
| **ECS** | hecs 实体组件系统，数据与逻辑解耦（Phase 1 保留但不参与运行时渲染） |
| **WGSL** | WebGPU Shading Language，统一着色器语言 |
| **RON** | Rusty Object Notation，场景描述格式。所有场景用 `.ron` 文件定义 |
| **MRT** | Multiple Render Targets，GBufferPass 同时写入 4 个纹理 |
| **CSM** | Cascaded Shadow Maps，级联阴影 |
| **IBL** | Image-Based Lighting，基于图像光照 |
| **FlyCam** | UE 风格自由飞行相机：右键按住激活，WASD 沿视线移动，鼠标控制 pitch/yaw，滚轮调速。所有场景统一使用 FlyCam |
| **DebugGizmo** | 世界原点 RGB 三轴指示器（红X/绿Y/蓝Z），带箭头，长度 ~0.15 单位 |
| **DebugGrid** | XZ 平面参考网格，10×10 单位，1 单位间距，无渐隐 |

## 架构约束

- 每个模块 < 500 LOC，适合单次 AI 生成
- 无复杂泛型约束
- Trait 接口清晰：`Pass`、`System`、`Asset`
- wgpu 自动适配后端（Vulkan/Metal/DX12），无 `#ifdef`

## 技术演进路线

```
Phase 0: Skeleton (window, triangle, egui)         ← ✅ 已完成
Phase 1: Deferred PBR + Shadow + Pipeline + RON + IBL ← ✅ 已完成
Phase 2: SSAO + SSR                                    ← 🔲 计划中
Phase 3: Editor (ECS runtime, Picking, Gizmo, UI)  ← 🔲 计划中
Phase 4: Post-process chain + Tone mapping
Phase 5: Terrain + Atmosphere + Water + Clouds
Phase 6: Ray Tracing (Compute + Hybrid)
```

## 已知陷阱

- **wgpu MRT**: 不支持多 fragment entry points，必须用单 `fs_main` 返回 `FragmentOutput` struct
- **Windows include_str!**: 可能产生 ghost shader 文件，用 `r#"..."#` + `Cow::Borrowed` 内联
- **Normal 编码**: GBuffer `*0.5+0.5`，Lighting `*2.0-1.0`
- **Surface `'static`**: `Arc<Window>` 需满足生命周期
- **全屏四边形 UV 翻转**: wgpu NDC Y=1 是顶，纹理 UV=0 也是顶，全屏四边形 `position * 0.5 + 0.5` 会导致 G-Buffer 垂直翻转采样。正确公式：`uv = vec2(x*0.5+0.5, 0.5 - y*0.5)`
- **FlyCam right() 叉积顺序**: 正确 = `forward × world_up`（右向量），错误 = `world_up × forward`（左向量），导致 A/D 反向
- **Render pass 内 queue.write_buffer**: Metal 上跨 draw call 的 uniform buffer 写入不可靠——所有 per-object 数据必须在 render pass 开始前 pre-upload，用 dynamic uniform offset 切换
- **Shadow map Z 映射**: `glam::Mat4::orthographic_rh` 输出 OpenGL 约定 z∈[-1,1]，但 wgpu/Vulkan depth buffer 使用 z∈[0,1]。必须自写 wgpu 兼容的正交矩阵（`-1/(f-n)` + `-n/(f-n)`）
- **Shadow UV Y-flip**: NDC y=-1 对应纹理 y=0（顶部），需 `uv.y = 0.5 - ndc.y*0.5`（和全屏 quad 同样规则）
- **球体绕序**: `sphere()` 默认索引顺序 `(base, base+s+1, base+1)` 产生 CW 绕序（from outside），导致 inside-out。正确顺序 `(base+s+1, base, base+1)`
- **IBL cubemap 投影**: `capture_projection()` 必须用 `correction * p_gl`（先 GL 投影再 z 修正），且 x/y 要同步缩放 2x 补偿 w 的变化。详见 ADR-0006。
- **HDR 加载翻转**: `image` crate 加载 HDR 原点在左上角，equirect 贴图原点在左下角 → 图像垂直翻转。通过 `capture_views` 的 ±Y 面对调补偿，IBL 反射采样时 Y 取反。详见 ADR-0006。
- **天空盒视线重建**: `world_ray.xyz / world_ray.w` 是从世界原点出发的方向，必须减 `camera_pos` 才是相机视线方向。
- **天空盒色调映射**: 天空路径和几何路径必须共用同一个 tone mapping，不能各自独立 return。
- **鼠标 Delta 累加**: `InputManager` 的 `mouse_delta` 必须 `+=` 累加，不能 `=` 覆盖（winit 每帧多次 CursorMoved 事件）。
- **Per-object draw 顺序**: GBufferPass/ShadowPass 在 render pass 内逐物体 draw 时，若用 `queue.write_buffer` 更新 uniform，部分物体可能拿到 stale 数据。方案：pre-upload 全部 per-object 数据到 dynamic uniform buffer，render pass 内仅 `set_bind_group(offset)` + draw
- **Shadow bias**: 使用软件 slope-scale bias（见 ADR-0004）。公式 `tan(acos(NdotL))`，base=0.005 NDC 单位，clamp 到 base×10。硬件 `DepthBiasState` 保持全零（Depth32Float 精度不足以在当前投影尺度下调参）。不使用 world-space 法线偏移（对垂直于光源的表面无效），不使用 front-face culling（导致漏光）。
- **Depth-only 渲染**: 不需要 fragment shader。`fragment: None` + vertex shader 只输出 `@builtin(position)`，GPU 自动从 clip_position 推导深度。手动写 `@builtin(frag_depth)` 容易出错（如 return 0.0）

## 关键文件

| 文件 | 职责 |
|------|------|
| `crates/aether-engine/src/scene/` | 场景描述数据结构 + SceneLoader（RON → GPU 资源） |
| `crates/aether-engine/src/renderer/passes/gbuffer.rs` | G-Buffer Pass (MRT) |
| `crates/aether-engine/src/renderer/passes/lighting.rs` | Deferred Lighting Pass |
| `crates/aether-engine/src/renderer/passes/debug.rs` | DebugGrid + DebugGizmo 线框渲染 |
| `crates/aether-engine/src/renderer/camera.rs` | FlyCamera（唯一相机控制器） |
| `crates/aether-engine/src/asset/` | 资源管理（Mesh/Texture/Material）+ BuiltinMeshRegistry |
| `crates/aether-launcher/src/main.rs` | Launcher 入口：管线编排、场景发现、event loop |
| `scenes/` | RON 场景文件目录，Launcher 自动扫描 |

## 决策记录

见 `docs/adr/` 目录。
