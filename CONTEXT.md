# AetherEngine — 领域上下文

## 项目定位

Rust + wgpu 现代渲染引擎，KongEngine 的精神续作。目标：从 Deferred PBR 学到实时光追。

## 核心领域语言

| 术语 | 含义 |
|------|------|
| **RenderGraph** | 声明式渲染管线编排，Pass 依赖自动排序 |
| **G-Buffer** | 延迟渲染几何缓冲（Position/Normal/Albedo/Material） |
| **RenderPass** | 单一渲染阶段 trait，实现 `prepare()` + `execute()` |
| **ECS** | hecs 实体组件系统，数据与逻辑解耦 |
| **WGSL** | WebGPU Shading Language，统一着色器语言 |
| **RON** | Rusty Object Notation，场景序列化格式 |
| **MRT** | Multiple Render Targets，GBufferPass 同时写入 4 个纹理 |
| **CSM** | Cascaded Shadow Maps，级联阴影 |
| **IBL** | Image-Based Lighting，基于图像光照 |

## 架构约束

- 每个模块 < 500 LOC，适合单次 AI 生成
- 无复杂泛型约束
- Trait 接口清晰：`RenderPass`、`System`、`Asset`
- wgpu 自动适配后端（Vulkan/Metal/DX12），无 `#ifdef`

## 技术演进路线

```
Phase 0: Skeleton (window, triangle, egui)     ← 当前
Phase 1: Deferred PBR + Shadows + IBL
Phase 2: SSR + SSAO + Post-Process
Phase 3: Terrain + Atmosphere + Water + Clouds
Phase 4: Ray Tracing (Compute + Hybrid)
```

## 已知陷阱

- **wgpu MRT**: 不支持多 fragment entry points，必须用单 `fs_main` 返回 `FragmentOutput` struct
- **Windows include_str!**: 可能产生 ghost shader 文件，用 `r#"..."#` + `Cow::Borrowed` 内联
- **Normal 编码**: GBuffer `*0.5+0.5`，Lighting `*2.0-1.0`
- **Surface `'static`**: `Arc<Window>` 需满足生命周期

## 关键文件

| 文件 | 职责 |
|------|------|
| `crates/aether-engine/src/renderer/passes/gbuffer.rs` | G-Buffer Pass (MRT) |
| `crates/aether-engine/src/renderer/passes/lighting.rs` | Deferred Lighting Pass |
| `crates/aether-engine/src/renderer/graph.rs` | RenderGraph 编排 |
| `crates/aether-engine/src/scene/loader.rs` | 场景加载/实例化 |
| `crates/aether-engine/src/asset/` | 资源管理（Mesh/Texture/Material） |

## 决策记录

见 `docs/adr/` 目录。
