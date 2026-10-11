<p align="center">
  <img src="docs/images/aether-engine-banner.svg" alt="Aether Engine — 渲染、检查、重放" width="100%">
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a>
  <br><br>
  <img alt="Rust 2021" src="https://img.shields.io/badge/Rust-2021-303846?style=flat-square&logo=rust&logoColor=white">
  <img alt="wgpu" src="https://img.shields.io/badge/GPU-wgpu-80cbc4?style=flat-square">
  <img alt="MIT 或 Apache--2.0 许可证" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-64748b?style=flat-square">
  <img alt="开发中" src="https://img.shields.io/badge/status-开发中-f0b76f?style=flat-square">
</p>

**Aether Engine 是一个 Rust 实时 3D 引擎，我们希望让每一帧都能解释自己。**你可以打开场景，检查渲染 Pass 如何依赖 GPU 资源，在编辑器里修改场景，再用固定时间重放模拟。

这是一个持续开发中的引擎研发项目，尚不是生产就绪的通用引擎。当前重点是非光追实时渲染、可操作的场景编辑器、确定性模拟，以及让代码变更更容易复核的验证证据。

<p align="center">
  <a href="#运行一个场景">运行一个场景</a> ·
  <a href="#aether-的独特点">Aether 的独特点</a> ·
  <a href="docs/engine/README.md">引擎文档</a> ·
  <a href="docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html">项目路线图</a>
</p>

## 看看引擎在渲染什么

<table>
  <tr>
    <td width="50%"><img src="docs/images/showcase-materials.png" alt="基于图像的光照下的 PBR 材质网格" width="100%"><br><sub><b>材质</b> · 在 IBL 环境中比较 PBR 响应</sub></td>
    <td width="50%"><img src="docs/images/showcase-lighting.png" alt="方向光和阴影照亮彩色物体" width="100%"><br><sub><b>光照</b> · 同时观察材质、灯光和阴影</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/showcase-clouds.png" alt="地平线上空的体积云" width="100%"><br><sub><b>大气</b> · 地形上空的体积云</sub></td>
    <td width="50%"><img src="docs/images/showcase-physics-ramp.png" alt="斜坡上的刚体物理模拟" width="100%"><br><sub><b>模拟</b> · 斜坡上的刚体运动</sub></td>
  </tr>
</table>

## Aether 的独特点

### 可检查的渲染流程

每个 Pass 都声明自己读取和写入的 GPU 资源。Pipeline Builder 根据声明安排执行顺序，并通过带类型的资源句柄连接 Pass。依赖缺失或形成环时，可以从管线契约和图示证据中定位，而不必只靠观察最终画面。

![渲染 Pass 与资源流向](docs/images/rendering-overview.svg)

### 从编辑器到 GPU 使用同一份场景

场景从可读的 RON 文件载入 `hecs` 世界。编辑器修改这份场景数据；渲染前的 Extract 阶段将它转换成不可变的帧数据。相同的场景可以保存、重开，也可以作为专项示例或验证用例。

### 时间是测试输入的一部分

物理和粒子使用固定步长推进。动画支持显式 seek 时间并发布帧快照，因此一个姿势可以重复播放和比较。T7 加入了 owned glTF 数据、四权重 GPU 蒙皮、128-joint palette，以及对不支持或超限 skin 的逐 primitive 回退。

### 视觉结果附带验证证据

VisualCase 固定场景、相机、时间和功能开关。报告会保留渲染图与差异图，以及 RenderGraph、指标、哈希和进程诊断。截图能展示结果变化；用例契约和报告还能帮助解释变化原因。

### GPU 资源有明确的所有权边界

资源 worker 只解码 CPU 数据。Launcher 在帧边界提交 GPU 资源，并通过 asset generation 丢弃过期结果；reload 失败时，原有可用资源仍能继续使用。

当前渲染器覆盖延迟 PBR、IBL、方向光与局部光、阴影、透明、后处理、地形、水体、大气和体积云。SSAO 与 SSR 是屏幕空间效果，只能使用当前相机可见的信息；光线追踪不在项目范围内。

## 当前项目进度

**更新于 2026-10-11。**路线图将 T7 标为已验收，T8 资产生命周期、材质、Prefab 和热重载工作标为完成，T9 标为已验收。T9.4 仅保留为研究结论，生产路径为 NO-GO。路线图还记录了 T0–T6 尚待完成的审查项及其验证证据。

切片状态、待处理验证和证据链接见[路线图](docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html)。项目 CI 入口为 `./scripts/verify-ci.sh`；只有该命令成功运行后才表示 CI 验证通过，路线图验收状态本身不代表 CI 已通过。

## 运行一个场景

需要稳定版 Rust 工具链（workspace 声明 Rust 1.78）以及 `wgpu` 支持的 GPU 和图形后端。

```bash
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine
cargo run -p aether-launcher
```

启动后会打开场景浏览器。也可以直接进入专项场景：

```bash
# PBR 材质网格
cargo run -p aether-launcher -- --scene scenes/24_t3_pbr_material_grid.ron

# 体积云
cargo run -p aether-launcher -- --scene scenes/13_clouds.ron

# 骨骼动画
cargo run -p aether-launcher -- --scene scenes/t7_gltf_actor.ron
```

## 按主题探索

| 从这里开始 | 内容 |
| --- | --- |
| [架构](docs/engine/architecture.md) | ECS、帧数据提取和基于资源声明的渲染调度 |
| [渲染](docs/engine/rendering.md) | PBR、光照、阴影、SSR/SSAO、水体、地形和体积云 |
| [编辑器](docs/engine/editor.md) | 场景浏览、Hierarchy、Inspector、变换编辑和保存 |
| [模拟](docs/engine/simulation.md) | 物理、粒子、播放控制和固定步长 |
| [场景与资源](docs/engine/scenes-assets.md) | RON 场景、模型、纹理和资源路径 |
| [路线图](docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html) | 当前里程碑、切片和验收证据 |
| [工程规范](docs/engineering-governance.md) | 代码健康度与必需验证 |
| [视觉测试流程](docs/agents/visual-test-workflow.md) | 可重复截图、报告和回归检查 |

## 运行项目检查

```bash
./scripts/verify-ci.sh
```

修改渲染后，还应运行对应场景回归，例如：

```bash
./scripts/verify-regression.sh --scene 13_clouds
```

Workspace 使用 [`MIT OR Apache-2.0`](Cargo.toml) 许可证。
