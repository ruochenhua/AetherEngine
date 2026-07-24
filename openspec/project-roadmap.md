# Aether Engine 项目路线图

## 项目当前状态

**已完成的骨架**：
- ✅ Cargo workspace 配置
- ✅ 项目目录结构
- ✅ OpenSpec 工作流配置
- ✅ 基础模块骨架（ecs, scene, asset, renderer, physics, input, math, window）
- ✅ Git 仓库初始化
- ✅ 所有渲染 Pass 和示例程序（Phase 0–2）
- ✅ 编辑器基础设施（Phase 3 完成）
- ✅ 后处理链（Phase 4 完成）

---

## 长期愿景

成为 KongEngine 的精神续作——一个架构现代、AI 协作友好、功能完整的个人渲染引擎，
覆盖从 Deferred PBR 到实时光追的完整渲染管线。

---

## 阶段规划

### Phase 0: 引擎骨架 ✅

**目标**：可编译、可运行的最小窗口程序

| 功能 | 状态 | 优先级 |
|------|------|--------|
| wgpu + winit 初始化 | ✅ 已完成 | P0 |
| 最小 RenderGraph | ✅ 已完成 | P0 |
| ECS World 封装 | ✅ 已完成 | P0 |
| 黑色清屏 | ✅ 已完成 | P0 |
| egui 集成 | ✅ 已完成 | P0 |

**待完成 OpenSpec Changes**：无

### Phase 1: 基础 Deferred PBR（核心）✅

**目标**：能加载场景，显示多光源 PBR 渲染结果

| 功能 | 状态 | 优先级 |
|------|------|--------|
| RON 场景加载 | ✅ 已完成 | P0 |
| G-Buffer Pass | ✅ 已完成 | P0 |
| Deferred Lighting（PBR） | ✅ 已完成 | P0 |
| IBL（Skybox + 环境贴图） | ✅ 已完成 | P0 |
| 阴影（CSM + PCF） | ✅ 已完成 | P0 |
| FlyCam 相机 | ✅ 已完成 | P0 |

**待完成 OpenSpec Changes**：无

### Phase 2: 屏幕空间效果 ✅

**目标**：SSAO、SSR

| 功能 | 状态 | 优先级 |
|------|------|--------|
| SSAO | ✅ 已完成 | P1 |
| SSR（基础光线行进） | ✅ 已完成 | P1 |
| SSR 性能调优 / 视锥裁剪 | ✅ 已完成 | P1 |
| SSR 深度采样重构（#39，已回退） | ❌ 已放弃 | P1 |

### Phase 3: 编辑器基础设施 ✅

**目标**：ECS runtime、编辑器交互、场景持久化

| 功能 | 状态 | 优先级 |
|------|------|--------|
| ECS runtime + Extract System | ✅ 已完成 | P1 |
| Ray picking（射线拾取） | ✅ 已完成 | P1 |
| Transform gizmo（平移/旋转/缩放） | ✅ 已完成 | P1 |
| Editor UI shell（egui 面板） | ✅ 已完成 | P1 |
| Scene hierarchy（场景层级） | ✅ 已完成 | P1 |
| Inspector（属性编辑） | ✅ 已完成 | P1 |
| Scene save/load（RON 序列化，Open/Import 语义） | ✅ 已完成 | P1 |
| Undo / Redo | ✅ 已完成 | P2 |
| Delete 选中物体 | ✅ 已完成 | P2 |
| 多选（Shift/Ctrl） | ✅ 已完成 | P2 |
| Fullscreen 切换 | ✅ 已完成 | P2 |

### Phase 4: 后处理链 ✅

**目标**：可配置的后处理管线

| 功能 | 状态 | 优先级 |
|------|------|--------|
| Post-process chain（ToneMap→FXAA→DebugLine） | ✅ 已完成 | P2 |
| Tone mapping（ACES / Reinhard / Off） | ✅ 已完成 | P2 |
| Bloom（8-pass multi-mip extract→upsample） | ✅ 已完成 | P2 |
| FXAA（3.11 quality presets Low/Med/High） | ✅ 已完成 | P2 |
| GPU Instancing（per-batch instanced draw） | ✅ 已完成 | P2 |
| SSAO View-Space 修复 + Bilateral Blur | ✅ 已完成 | P2 |

### Phase 4.5: 大场景前置基础设施 ✅

**目标**：为地形渲染铺好最小必要的基础能力

| 功能 | 状态 | 优先级 |
|------|------|--------|
| 场景格式扩展（RON `terrain` 字段 + TerrainSource 抽象） | ✅ 已完成 (#75) | P2 |
| AABB / Bounding Volume 统一 | ✅ 已完成 (#76) | P2 |
| 视锥裁剪（Frustum Culling） | ✅ 已完成 (#76) | P2 |
| 地形材质 Splatting 基础 | ✅ 已完成 (#77) | P2 |

### Phase 5: 大场景渲染 ✅

**目标**：地形、大气、水体、体积云

| 功能 | 状态 | 优先级 |
|------|------|--------|
| 地形（Chunked LOD + 可选 Pass + GBuffer 集成） | ✅ 已完成 (#78) | P3 |
| 异步 Asset 加载 | ✅ 已完成 (#79) | P2 |
| CSM 级联阴影 | ✅ 已完成 (#80) | P3 |
| GPU Timer 性能面板 | ✅ 已完成 (#81) | P3 |
| 物理大气散射 | ✅ 已完成 (#82) | P3 |
| 水体（Gerstner 波） | ✅ 已完成 (#83) | P3 |
| 体积云（Ray marching） | ✅ 已完成 (#84) | P3 |
| God Ray | ✅ 已完成 (#85) | P3 |

### Phase 6: 打磨与工程健康度 🔲 当前

**目标**：暂缓新功能，把现有能力打磨扎实——工程健康度 + 文档一致性

| 功能 | 状态 | 优先级 |
|------|------|--------|
| 完善单元测试 | ✅ 已完成（263→280 个，P0/P1 缺口补齐） | P3 |
| Shader 编译错误处理 | ✅ 已完成 | P3 |
| GPU 内存泄漏检查 | ✅ 已完成 | P3 |
| PipelineBuilder transient 资源复用优化 | 🔲 待实现 | P4 |
| 多线程 System 执行 | 🔲 待实现 | P4 |
| 编辑器多选（Shift/Ctrl） | ✅ 已完成 | P4 |
| Camera speed 持久化 | ✅ 已完成（早已实现，本轮补测试） | P4 |
| README / CONTEXT 与实际管线对齐 | ✅ 已完成 | P4 |

**备注**：光线追踪（Compute Path Tracer / Hybrid RT / Denoising）已暂缓，待打磨阶段完成后重新评估。

### Phase 7: 物理与动画（远期）

**目标**：接入物理系统

| 功能 | 状态 | 优先级 |
|------|------|--------|
| rapier3d 接入 | 🔲 待实现 | P5 |
| 基础动画系统 | 🔲 待实现 | P5 |
| 粒子系统 | 🔲 待实现 | P5 |

---

## 示例程序路线图

| 示例 | 验证功能 | 对应 Phase |
|------|---------|-----------|
| `01_deferred` | G-Buffer + Lighting + Shadow | Phase 1 |
| `02_multi_object` | 多物体 + 多光源 | Phase 1 |
| `03_shadow_demo` | CSM + PCF 阴影展示 | Phase 1 |
| `04_ibl_debug` | Skybox + IBL 调试用 | Phase 1 |
| `05_ssao_debug` | SSAO 调试 | Phase 2 |
| `06_ssao_extreme` | SSAO 极端场景 | Phase 2 |
| `07_ssr_debug` | SSR 调试 | Phase 2 |
| `08_postprocess` | Bloom + Tone mapping | Phase 4 |
| `09_terrain` | 地形 + 曲面细分 | Phase 5 |
| `10_water` | 水体 | Phase 5 |
| `11_volumetric` | 体积云 | Phase 5 |
| `12_raytrace` | 光追（暂缓，见 Phase 6 备注） | 暂缓 |

---

## 技术债务与优化

### 高优先级
- [ ] 完善单元测试
- [ ] Shader 编译错误处理
- [ ] GPU 内存泄漏检查

### 中优先级
- [ ] PipelineBuilder transient 资源复用优化
- [x] Asset 异步加载
- [ ] 多线程 System 执行

### 低优先级
- [x] 性能分析面板
- [ ] 远程调试支持

---

## 版本规划

### v0.1.0 - 骨架版 ✅
- ✅ 项目结构
- ✅ 基础模块
- ✅ 最小可运行（Launcher + 场景切换）

### v0.2.0 - Deferred 版 ✅
- ✅ G-Buffer + Lighting
- ✅ Shadow Mapping
- ✅ IBL + Skybox
- ✅ SSAO + SSR

### v0.3.0 - 编辑器版 ✅
- ✅ ECS runtime
- ✅ Ray picking
- ✅ Transform gizmo（平移/旋转/缩放）
- ✅ Editor UI shell
- ✅ Undo / Redo / Delete
- ✅ Scene save/load（Open/Import/Save，RON 格式）

### v0.4.0 - 后处理版 ✅
- ✅ Post-process chain
- ✅ Tone mapping（ACES/Reinhard/Off）
- ✅ Bloom（8-pass）
- ✅ FXAA（3.11）
- ✅ GPU Instancing
- ✅ SSAO View-Space + Bilateral Blur

### v0.5.0 - 大场景版 ✅
- ✅ Terrain
- ✅ Atmosphere
- ✅ Water
- ✅ Clouds
- ✅ God Rays
- ✅ CSM
- ✅ GPU Timer

### v0.6.0 - 打磨版 🔲 当前
- 单元测试补全
- Shader 编译错误处理
- GPU 内存泄漏检查
- PipelineBuilder transient 资源复用
- 多线程 System 执行
- 编辑器遗留项（多选、Camera speed 持久化）
- 文档与管线对齐

（原 v0.6.0 光追版暂缓，待打磨完成后重新规划）

---

## 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|----------|
| wgpu API 变更 | 中 | 锁定版本，定期升级 |
| AI 生成代码质量不稳定 | 高 | 模块化设计，每个文件独立验证 |
| 性能瓶颈 | 中 | 示例程序逐步验证，先正确再优化 |
| 功能范围膨胀 | 高 | 严格按 Phase 执行，非核心功能延后 |
