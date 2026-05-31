# PRD: Phase 2 — IBL, SSAO, SSR

## Problem Statement

当前引擎只有单方向光 + 固定 ambient 的环境光照，画面缺乏环境光细节、遮蔽感、反射质感。场景中物体看起来漂浮在空洞的背景中，材质（金属/粗糙度）的差异体现不出来。

## Solution

Phase 2 为延迟渲染管线增加三个屏幕空间 / 基于图像的光照特性：

1. **IBL（Image-Based Lighting）** — 从 HDR 环境贴图计算漫反射和镜面反射环境光，替代写死的 ambient 值，让材质粗糙度/金属度差异化可见
2. **SSAO（Screen-Space Ambient Occlusion）** — 在物体接触处、凹陷处增加遮蔽阴影，增强立体感和场景深度
3. **SSR（Screen-Space Reflections）** — 光滑表面反射周围物体的屏幕空间光线追踪，补全 IBL 无法反映的局部反射

## User Stories

1. As a user, I want the scene to have a sky/environment lighting, so that objects are not lit by a flat ambient color
2. As a user, I want rough surfaces (like concrete) to show diffuse environment reflections and smooth surfaces (like metal) to show sharp reflections, so that the PBR material model is fully realized
3. As a user, I want objects in contact to cast subtle occlusion shadows (e.g. cube sitting on a plane), so that the scene has depth and grounding
4. As a user, I want to see real-time reflections of nearby geometry on shiny surfaces, so that the scene feels physically connected
5. As a developer, I want each new pass to follow the same Pass trait + signature pattern as existing passes, so that adding/modifying them is straightforward
6. As a developer, I want each feature to be independently toggleable via debug modes in the launcher, so that I can isolate visual issues

## Implementation Decisions

### Architecture

所有三个特性遵循现有 Pass 模式：
- 各自一个独立 Pass 文件：`passes/ibl.rs`、`passes/ssao.rs`、`passes/ssr.rs`
- 声明 `PassSignature`（reads G-Buffer textures，writes 各自输出纹理）
- 在 LightingPass 中整合所有输出
- PipelineBuilder 自动处理拓扑排序和纹理分配

### IBL

**输入**：HDR equirectangular 环境贴图（`.hdr` 文件，通过 `image` crate 加载）
**输出**：三张纹理
- `IrradianceMap`（R16G16B16A16Float cubemap，32×32 per face）：漫反射辐照度，对余弦lobe做卷积
- `PrefilteredMap`（R16G16B16A16Float cubemap，128×128 base + 5 mip levels）：镜面反射预过滤，每级 mip 对应一个粗糙度
- `BrdfLUT`（RG16Float 2D，256×256）：BRDF 积分查找表

**实现方式**：Compute shader。在 `init()` 阶段加载环境贴图并通过 compute dispatch 生成上述纹理。LightingPass 的 bind group 新增这三个纹理 + 对应 sampler。Lighting shader 中新增 IBL 采样代码。

**新增 ResourceTag**：`IrradianceMap`, `PrefilteredMap`, `BrdfLUT`

### SSAO

**输入**：G-Buffer world position + normal
**算法**：参考 Unreal Engine 4 的 SSAO 方案
- 屏幕空间随机采样半球 + 深度比较
- 4×4 随机旋转向量纹理（`R8G8B8A8Unorm`）
- 可选：双边模糊 pass（复用或就地 blur）

**输出**：`AOTexture`（`R8Unorm`，全分辨率），LightingPass 已声明 `AOTexture` 读依赖

**实现方式**：全屏四边形 pass，fragment shader 计算 AO。独立 Bloom/Blur 不在 Phase 2 范围内——先用简单的 4-tap 就地模糊。

### SSR

**输入**：G-Buffer world position、normal、albedo、material（roughness/metallic）
**算法**：屏幕空间光线行进（Hi-Z 加速可选，Phase 2 先用 brute-force raymarch）
- 反射向量从 world normal + camera view 计算
- 沿反射方向步进，每一步检查深度是否命中
- 命中后采样 albedo 作为反射颜色
- 按 roughness 混合：rough 表面采样 prefiltered IBL 替代

**输出**：`ReflectionTexture`（`R11G11B10Float`，半分辨率以节省性能）

### 管线变更

```
PipelineBuilder
  ├── ShadowPass         → writes: ShadowDepth
  ├── GBufferPass        → writes: GPosition, GNormal, GAlbedo, GMaterial, GDepth
  ├── IBLPass            → reads: (none from G-Buffer) → writes: IrradianceMap, PrefilteredMap, BrdfLUT
  ├── SSAOPass           → reads: GPosition, GNormal   → writes: AOTexture
  ├── SSRPass            → reads: GPosition, GNormal, GAlbedo, GMaterial → writes: ReflectionTexture
  ├── LightingPass       → reads: GPosition, GNormal, GAlbedo, GMaterial,
  │                         ShadowDepth, IrradianceMap, PrefilteredMap, BrdfLUT,
  │                         AOTexture, ReflectionTexture → writes: Swapchain
  └── DebugLinePass      → reads: GDepth → writes: Swapchain (LoadOp::Load)
```

### LightingPass 整合

Lighting shader 的最终颜色公式（Phase 2 后）：
```
final_color = direct_lighting * shadow
            + diffuse_ibl * albedo * ao
            + specular_ibl * (F0 + F1 * brdf_lut) * ao
            + ssr_reflection * (1.0 - ssr_fade)
```
其中 `direct_lighting = ambient + diffuse + specular`（现有公式），`ambient` 项被 IBL 替代后移除或降低权重。

## Testing Decisions

遵循 TDD 原则：

### 测试范围
- 每个新 Pass 的 `signature()` 正确声明读写依赖（类型安全）
- PipelineBuilder 集成测试：所有 7 个 pass 能成功 build
- IBL：compute shader 生成的 cubemap 尺寸和 mip 级别正确
- SSAO/SSR：输出纹理格式正确

### 测试方法
- 复用 `scheduler.rs` 中的集成测试模式（`build_all_passes_works`）
- 各 Pass 的 signature 单元测试（参考 `pass.rs` 中的 `signature_reads_and_writes`）
- 使用 `headless_device()` 创建测试用 wgpu device（已有）

### 测试优先级
- RED-GREEN-REFACTOR 每个 issue 独立完成
- Issue 1 (IBL): tests pass → merge
- Issue 2 (SSAO): rebase on main → tests pass → merge
- Issue 3 (SSR): rebase on main → tests pass → merge

## Out of Scope

- 实时动态环境贴图更新（IBL 在 init 时加载一次）
- HDR 环境贴图烘焙工具（手动提供 `.hdr` 文件）
- SSAO 的时序抗锯齿（TAA）
- SSR 的 Hi-Z 加速
- 后处理（Phase 4）
- 多光源 IBL/阴影（当前只有单方向光）

## Further Notes

- IBL 的 compute shader 是引擎第一个 compute pass，需要验证 wgpu compute pipeline 在 Metal 后端（Intel Mac）上的兼容性
- AOTexture tag 已在 `resource.rs` 中预定义，SSAOPass 可以直接使用
- 现有 debug 模式（按键 0-6）保持不变，新增 debug 模式：7=IBL only, 8=SSAO, 9=SSR
