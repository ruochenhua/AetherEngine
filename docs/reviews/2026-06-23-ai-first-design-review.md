# AetherEngine 代码设计评审报告

- **评审日期：** 2026-06-23
- **评审范围：** AetherEngine 整体代码架构与设计，重点评估其 "AI-first" 设计目标的实现程度
- **评审对象：** `crates/aether-engine/src/`、`crates/aether-launcher/src/`
- **Git 范围（参考）：** `fc769570..e7ee141a`
- **评审结论：** 当前设计不适合直接继续扩展，需先修复基础架构问题

---

## 执行摘要

AetherEngine 作为一个以 AI Agent 为核心开发者的 3D 渲染引擎，在文档、测试覆盖、类型安全资源句柄等方面表现良好。然而，核心架构存在多处与 "AI-first" 宣称不符的设计缺陷：

- `Pass` trait 的三阶段生命周期（`init` / `resolve` / `execute`）与实际实现严重脱节
- `Scheduler` 通过 `as_any_mut` 与每个具体 Pass 类型深度耦合
- `RenderContext` 使用不安全的 `transmute` 伪造 `'static` Surface 生命周期
- 核心模块远超宣称的 500 行限制
- "构建期错误" 实为运行时 panic

这些问题会在 Phase 6 光线追踪阶段被进一步放大。建议在继续扩展前优先修复。

---

## 优点

| 项目 | 说明 |
|------|------|
| 测试覆盖扎实 | `cargo test --workspace --lib` 通过 170 个单元测试，覆盖调度器拓扑、资源表查询、场景序列化、相机数学、地形几何等 |
| 类型安全资源句柄 | `ResHandle<T>` 在编译期防止 `GPosition`/`GNormal` 等资源语义混淆 |
| Extract 阶段解耦 | `renderer/extract.rs` 在 Pass 运行前按 `(mesh, material)` 批处理渲染对象，符合 ADR-0008 意图 |
| Pipeline 拓扑排序有效 | `pipeline_builder.rs` 能检测缺失生产者和循环依赖，并有测试验证 |
| 文档完善 | `CLAUDE.md`、`CONTEXT.md` 及 11 份 ADR 提供了清晰约定和陷阱清单 |
| Pass 模板可用 | `renderer/passes/template.rs` 是具体的 AI 开发模板 |
|  crate 边界清晰 | `aether-engine` 为库，`aether-launcher` 为薄编排层 |

---

## 关键问题（Critical / 必须修复）

### 1. `RenderContext` 使用 `unsafe transmute` 伪造 `Surface<'static>`

- **文件：** `crates/aether-engine/src/renderer/context.rs:33`
- **问题描述：**
  ```rust
  let surface = unsafe {
      std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(surface)
  };
  ```
  窗口由 `App` 中的 `Arc<Window>` 持有，并非 `'static` 生命周期。`wgpu::Surface<'static>` 要求底层窗口存活 `'static`。这种转换存在释放后使用的未定义行为风险，在窗口调整大小、关闭或多窗口场景下可能崩溃或产生内存损坏。
- **影响：** 图形上下文层面的 UB 是最底层风险，会影响整个引擎的稳定性。
- **修复建议：**
  - 方案 A：让 Surface 与 Window 的真实生命周期绑定，将两者共同存放在一个结构中。
  - 方案 B：构建一个持有 `Arc<Window>` 和 `Surface` 的上下文结构，让借用检查器自动保证生命周期安全。
  - **禁止**继续使用 `transmute` 擦除生命周期。

---

### 2. `Pass::init` 设计失效

- **文件：**
  - `renderer/passes/lighting.rs:67-71`
  - `renderer/passes/fxaa.rs:62`、`128`
  - `renderer/passes/tone_mapping.rs:60`、`127`
  - `renderer/passes/bloom/mod.rs:108`、`271`
  - `renderer/passes/volumetric_cloud.rs:79`、`225`
  - `renderer/passes/composite.rs:67`、`193`
  - `renderer/passes/debug.rs:86`
- **问题描述：**
  `Pass` trait 的签名为 `fn init(device: &wgpu::Device) -> Self`，但真实构造函数需要 `surface_format`、`depth_format`、`queue`、`width/height` 等更多参数。因此每个 Pass 都绕过 `init`，实现为 `Self::new(device)` 的占位形式，并暴露单独的 `new(...)` 供 Launcher 调用。`LightingPass::init` 甚至会创建占位 IBL 资源，忽略真实的 `IblResources`。
- **影响：** 广告宣传的三阶段生命周期（`init` → `resolve` → `execute`）是名存实亡的。AI 复制模板时会写 `init()`，但无法通过 trait 构造真正的 Pass，导致半初始化状态。
- **修复建议：**
  - 方案 A：扩展 `Pass::init` 接收一个上下文结构体：`InitContext { device, queue, surface_format, depth_format, width, height }`。
  - 方案 B：从 trait 中移除 `init`，让 Pass 通过各自的类型化 Builder 构造，然后装箱加入 `PipelineBuilder`。

---

### 3. 核心模块严重违反 <500 LOC 承诺

- **文件：**
  - `crates/aether-engine/src/scene/mod.rs:1249`
  - `crates/aether-engine/src/renderer/ibl.rs:1246`
  - `crates/aether-engine/src/renderer/passes/ssr.rs:1118`
  - `crates/aether-engine/src/renderer/passes/lighting.rs:904`
  - `crates/aether-engine/src/renderer/passes/water.rs:695`
  - `crates/aether-engine/src/renderer/passes/volumetric_cloud.rs:649`
  - 等至少 16 个引擎源文件超过 500 行
- **问题描述：**
  `CLAUDE.md:46-49` 和 `CONTEXT.md:47` 明确声明 "Each module < 500 LOC"，但最大文件已达 2.5 倍上限。
- **影响：** 这是项目最核心的 AI-first 宣称。超大文件使 AI 无法在单个上下文窗口内完整理解模块，违背设计初衷。
- **修复建议：**
  - `scene/mod.rs` → 拆分为 `scene_loader.rs`、`scene_description.rs`、`transform.rs` 等子模块
  - `ibl.rs` → 拆分为 `hdr_loader.rs`、`cubemap.rs`、`brdf_lut.rs`
  - `lighting.rs` → 将 WGSL 着色器字符串移入 `shaders/lighting.rs`
  - `ssr.rs` / `water.rs` / `volumetric_cloud.rs` → 按 "setup" / "execute" / "shaders" 拆分

---

### 4. `Scheduler` 与具体 Pass 类型深度耦合

- **文件：** `crates/aether-engine/src/renderer/scheduler.rs:85-217`
- **问题描述：**
  `Scheduler` 包含约 15 个 Pass 专用 setter：`set_ssao_params`、`set_bloom_params`、`set_tone_mapping_mode`、`set_fxaa_params` 等。这些 setter 通过 `Pass::as_any_mut` 进行向下转型。而 `Pass` trait 暴露 `as_any_mut`（`pass.rs:86-88`）solely 为了支持这种耦合。
- **影响：**
  新增一个需要每帧 UI 控制的 Pass，需要同时修改 `Pass` trait、`Scheduler` 和 Launcher。这正是 ADR-0003 声称要避免的 "edit 3–5 locations" 问题。
- **修复建议：**
  - 将 Pass 配置通过 `RenderFrame` 统一传递，例如引入 `FrameConfig` 结构体。
  - 或采用按 Pass 名称注册的 uniform/config 块。
  - **删除 `Pass::as_any_mut`** 和 Scheduler 中的所有专用 setter。

---

### 5. `RenderFrame` 暴露 `&World`，绕过 Extract

- **文件：** `crates/aether-engine/src/renderer/frame.rs:35`
- **问题描述：**
  ADR-0008 要求渲染阶段消费 Extract 输出的 `Vec<RenderBatch>`，而非直接查询 ECS。但 `RenderFrame` 仍携带 `world: &'a World`，且 `TerrainPass`、`AtmospherePass` 等直接查询 ECS。
- **影响：** 重新耦合了渲染与 ECS，AI 编写的新 Pass 很容易忽略 Extract 约定。
- **修复建议：**
  - 从 `RenderFrame` 中移除 `world` 字段。
  - Terrain、Cloud 等需要 ECS 数据的 Pass，应通过 Extract 输出或 per-pass data slot 接收数据。

---

### 6. "构建期错误" 实为运行时 panic

- **文件：**
  - `crates/aether-engine/src/renderer/pipeline_builder.rs:99-103`
  - `crates/aether-engine/src/renderer/pipeline_builder.rs:318-322`
  - `crates/aether-engine/src/renderer/pipeline_builder.rs:411-417`
- **问题描述：**
  缺失资源生产者和依赖循环在 `PipelineBuilder::build()` 时通过 `panic!` 终止，而不是返回 `Result`。文档称之为 "build-time errors"，但实质是运行时中止。
- **影响：**
  - 测试需要使用 `#[should_panic]`，无法验证具体错误类型。
  - Launcher 在 Pass 接线错误时会直接崩溃，无法通过 UI 友好提示。
- **修复建议：**
  - 将 `build()` 和 `compute_topological_order()` 改为返回 `Result<Scheduler, PipelineBuildError>`。
  - 定义 typed errors：`MissingProducer { pass: String, resource: String }`、`Cycle { path: Vec<String> }`。

---

### 7. `PipelineBuilder` 硬编码 `DebugLine` 排序

- **文件：** `crates/aether-engine/src/renderer/pipeline_builder.rs:234-238`
- **问题描述：**
  拓扑排序完成后，builder 手动将 `"DebugLine"` Pass 移动到队列末尾。
- **影响：** 这是与声明式调度宣称矛盾的魔法特例。新增覆盖类 Pass 时可能需要类似的硬编码。
- **修复建议：**
  - 在 Pass signature 中声明顺序约束（如 `must_run_after: &[&str]`）。
  - 或让覆盖 Pass 写入 swapchain 作为最终消费者，由资源依赖自然推导出最后执行。

---

## 重要问题（Important / 应该修复）

### 8. 纹理分配逻辑在 `build()` 和 `validate_and_allocate()` 中重复

- **文件：** `crates/aether-engine/src/renderer/pipeline_builder.rs:57-173`、`187-219`
- **问题描述：**
  生产者映射、依赖边、拓扑排序、纹理分配等逻辑在 `build()` 和 `validate_and_allocate()` 中重复出现，仅存在细微差异。
- **影响：** 违反 DRY，修复时容易遗漏其中一个路径。
- **修复建议：** 提取单一辅助函数 `validate_and_allocate(sigs: &[PassSignature]) -> ResourceTable`，供两个路径复用。

---

### 9. `Swapchain` 资源标签未使用

- **文件：** `crates/aether-engine/src/renderer/resource.rs:48-50`
- **问题描述：**
  存在 `Swapchain` 标签和 `ResourceTable::set_view`，但没有 Pass 写入 `Swapchain`，也没有代码调用 `set_view` 设置它。Pass 通过 `execute()` 参数接收 `surface_view`。
- **影响：** 未使用的 API 表面让资源模型变得混乱。
- **修复建议：**
  - 方案 A：将 swapchain 作为真正的 transient resource，由 `PresentPass` 生产。
  - 方案 B：移除 `Swapchain` 标签和 `set_view` 方法。

---

### 10. `IblResources::generate` 在 `queue` 为 `None` 时静默返回无效资源

- **文件：** `crates/aether-engine/src/renderer/ibl.rs:79-128`
- **问题描述：**
  函数签名为 `generate(device, queue: Option<&Queue>, config)`。当 `queue` 为 `None` 时跳过所有 GPU 工作，返回基于空/未初始化纹理的 view。该路径仅为了让测试能在没有 queue 的情况下构造 `IblResources`。
- **影响：** 生产 API 不应存在返回语义上无效资源的路径；测试需求不应泄漏到公共 API。
- **修复建议：**
  - 生产接口：`generate(device, queue, config) -> Self`，`queue` 必填。
  - 测试专用：`#[cfg(test)] fn placeholder(device, config) -> Self` 用于构造占位 cubemap。

---

### 11. Launcher 中 HDR 环境贴图路径硬编码

- **文件：** `crates/aether-launcher/src/pipeline.rs:30-37`
- **问题描述：**
  `build_pipeline` 硬编码 `"assets/hdr/newport_loft.hdr"`。
- **影响：** 场景无法指定自己的环境贴图，引擎忽略 `SceneDescription` 中的 IBL 设置。
- **修复建议：** 从场景描述或传入 `build_pipeline` 的配置结构体中读取环境贴图路径。

---

### 12. `SystemRegistry` / `System` trait 似乎未被使用

- **文件：**
  - `crates/aether-engine/src/ecs/system.rs`
  - `crates/aether-engine/src/ecs/mod.rs:29`
- **问题描述：**
  Launcher 以命令式方式驱动更新逻辑，没有代码向 `SystemRegistry` 注册系统。
- **影响：** 死抽象增加了表面积却没有收益。
- **修复建议：**
  - 如果 AI-first ECS 是目标，应在 Launcher 中真正采用 System scheduler。
  - 否则直接移除 `SystemRegistry` 和 `System` trait，减少认知负担。

---

### 13. 资源命名仍依赖字符串，类型安全只完成一半

- **文件：**
  - `crates/aether-engine/src/renderer/pass.rs:112-123`
  - `crates/aether-engine/src/renderer/resource_table.rs:72-88`
- **问题描述：**
  类型标签能防止 `GPosition`/`GNormal` 混淆，但逻辑名称仍是字符串。`"gbuffer_position"` 与 `"gbuffer_pos"` 这类拼写错误只能在运行时的 `ResourceTable::handle` panic 中被发现。
- **影响：** AI 编写 Pass 时容易因字符串拼写错误导致运行时崩溃。
- **修复建议：**
  - 为每个 tag 定义关联常量 `ResourceTag::NAME`。
  - 让 `signature().read::<GPosition>()` 自动推断对应名称，避免手动字符串。

---

### 14. Pass 模板与"一行注册"宣称矛盾

- **文件：** `crates/aether-engine/src/renderer/passes/template.rs:12-13`、`25-26`
- **问题描述：**
  模板要求开发者修改 `passes/mod.rs`、`main.rs`、Pass 列表，还要在 Launcher 中添加每帧 setter。
- **影响：** 文档记录了项目声称要消除的真实摩擦。
- **修复建议：**
  - 更新模板以匹配 "一行注册" 的目标。
  - 将每帧数据统一注入 `RenderFrame` 或 Pass config block，消除 Launcher 端的 setter 需求。

---

### 15. 屏幕尺寸状态分散在多个 setter 和 `rebuild()` 中

- **文件：**
  - `crates/aether-engine/src/renderer/scheduler.rs:86-111`
  - `crates/aether-engine/src/renderer/scheduler.rs:196-201`
  - `crates/aether-launcher/src/app.rs:401-405`
- **问题描述：**
  窗口调整大小时，Launcher 需要依次调用 `set_ssao_screen_size`、`set_ao_blur_screen_size`、`set_ssr_screen_size`、`set_bloom_screen_size`，然后再调用 `scheduler.rebuild()`。
- **影响：** 脆弱：新增一个依赖分辨率的 Pass 时，Launcher 可能忘记调用对应的 setter。
- **修复建议：**
  - 在 `rebuild()` 中统一接收 `(width, height)`，自动派生所有分辨率相关尺寸。
  - 或在 Scheduler 中存储当前尺寸，并在 `rebuild()` 内部自动传播。

---

## 次要问题（Minor /  Nice to Have）

16. **`SsrTraceResult` 缺少文档**
    - 文件：`crates/aether-engine/src/renderer/resource.rs:64`
    - 问题：触发 `#![warn(missing_docs)]` 警告。

17. **SSR 中未使用的 `reflection_view`**
    - 文件：`crates/aether-engine/src/renderer/passes/ssr.rs:255`

18. **`app/render.rs` 中的变量遮蔽**
    - 文件：`crates/aether-launcher/src/app/render.rs:107-108`
    - 问题：先声明 `mut extract_ms/apply_ms`，随后又在 `Running` 分支内用非 mut 声明遮蔽。

19. **部分内联 WGSL 字符串单行过长**
    - 文件：`crates/aether-engine/src/renderer/passes/gbuffer.rs:249`
    - 问题：影响可读性和 diff 友好性。

---

## 修复优先级建议

| 优先级 | 问题编号 | 说明 |
|--------|----------|------|
| P0 | 1, 2, 4, 5, 6 | 架构级问题，会影响后续所有 Pass 开发和稳定性 |
| P1 | 3, 7, 8, 9, 10, 11, 13, 14, 15 | 显著影响 AI-first 开发体验和代码健壮性 |
| P2 | 12, 16, 17, 18, 19 | 清理死代码、消除警告、提升可读性 |

---

## 总体评估

**当前设计是否适合继续扩展？** **否**

**理由：** `Pass` trait 的 `init` 方法在结构上已失效（真实构造函数需要远超 `&Device` 的参数），`Scheduler` 通过 `as_any_mut` 与每个具体 Pass 紧耦合，且 `RenderContext` 使用不安全的 `transmute` 伪造 `'static` surface。这些都是基础性问题，会在 Phase 6 光线追踪阶段快速放大。建议在推进新功能前先修复上述 Critical 和 Important 级别问题。

---

*本报告由代码评审子代理生成，经整理后存入项目文档。*
