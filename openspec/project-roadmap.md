# Aether Engine 项目路线图

## 项目当前状态

**已完成的骨架**：
- ✅ Cargo workspace 配置
- ✅ 项目目录结构
- ✅ OpenSpec 工作流配置
- ✅ 基础模块骨架（ecs, scene, asset, renderer, physics, input, math, window）
- ✅ Git 仓库初始化
- ✅ 所有渲染 Pass 和示例程序（Phase 0–2）
- ✅ 编辑器基础设施（Phase 3 进行中）

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

### Phase 3: 编辑器基础设施 ✅ 当前阶段

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
| 多选（Shift/Ctrl） | 🔲 待实现 | P2 |
| Fullscreen 切换 | ✅ 已完成 | P2 |

### Phase 4: 后处理链

**目标**：可配置的后处理管线

| 功能 | 状态 | 优先级 |
|------|------|--------|
| Post-process chain（可配置） | 🔲 待实现 | P2 |
| Tone mapping | 🔲 待实现 | P2 |
| Bloom / FXAA | 🔲 待实现 | P2 |
| GPU Instancing | 🔲 待实现 | P2 |

### Phase 5: 大场景渲染

**目标**：地形、大气、水体、体积云

| 功能 | 状态 | 优先级 |
|------|------|--------|
| 地形（高度图 + 曲面细分） | 🔲 待实现 | P3 |
| 物理大气散射 | 🔲 待实现 | P3 |
| 水体（Gerstner 波） | 🔲 待实现 | P3 |
| 体积云（Ray marching） | 🔲 待实现 | P3 |
| God Ray | 🔲 待实现 | P3 |

### Phase 6: 光线追踪

**目标**：从软件光追到硬件加速

| 功能 | 状态 | 优先级 |
|------|------|--------|
| Compute Shader Path Tracer | 🔲 待实现 | P4 |
| Hybrid Ray Tracing | 🔲 待实现 | P4 |
| Denoising | 🔲 待实现 | P4 |

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
| `12_raytrace` | 光追 | Phase 6 |

---

## 技术债务与优化

### 高优先级
- [ ] 完善单元测试
- [ ] Shader 编译错误处理
- [ ] GPU 内存泄漏检查

### 中优先级
- [ ] PipelineBuilder transient 资源复用优化
- [ ] Asset 异步加载
- [ ] 多线程 System 执行

### 低优先级
- [ ] 性能分析面板
- [ ] 远程调试支持
- [ ] 多选（Shift/Ctrl）
- [ ] Camera speed 持久化（当前 save/load 不保存 speed）

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

### v0.3.0 - 编辑器版 ✅ 当前
- ✅ ECS runtime
- ✅ Ray picking
- ✅ Transform gizmo（平移/旋转/缩放）
- ✅ Editor UI shell
- ✅ Undo / Redo / Delete
- ✅ Scene save/load（Open/Import/Save，RON 格式）

### v0.4.0 - 后处理版
- Post-process chain
- Tone mapping
- Bloom / FXAA
- GPU Instancing

### v0.5.0 - 大场景版
- Terrain
- Atmosphere
- Water
- Clouds

### v0.6.0 - 光追版
- Path Tracer
- Hybrid RT

---

## 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|----------|
| wgpu API 变更 | 中 | 锁定版本，定期升级 |
| AI 生成代码质量不稳定 | 高 | 模块化设计，每个文件独立验证 |
| 性能瓶颈 | 中 | 示例程序逐步验证，先正确再优化 |
| 功能范围膨胀 | 高 | 严格按 Phase 执行，非核心功能延后 |
