<p align="center">
  <img src="assets/branding/aether-engine-icon.png" alt="Aether Engine 图标" width="112">
  <br>
  <strong>AETHER ENGINE</strong>
</p>

<p align="center">
  <strong>一个便于探索渲染、场景编辑与模拟的实时 3D 引擎。</strong>
  <br>
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a>
</p>

---

Aether Engine 是一个使用 **Rust** 和 **wgpu** 开发的引擎项目，整合实时渲染、基于 ECS 的场景模型、交互式场景编辑器，以及可重复运行的物理和粒子模拟。项目仍在持续开发中，定位是引擎研发与探索项目，并非生产就绪的通用引擎。

## 项目特色

- **渲染流程便于检查：**渲染 Pass 声明读写的 GPU 资源，由调度器组织执行顺序。
- **编辑与渲染共用场景数据：**场景实体保存在 `hecs` ECS 世界中，再通过 Extract 阶段生成渲染数据。
- **模拟可重复运行：**物理基于 Rapier3D 固定步长更新；Launcher 提供 Play、Pause、Stop 控制。
- **专项场景便于观察：**用独立的 RON 场景探索不同渲染和模拟功能。
- **适合 Agent 协作迭代：**模块化功能和专项检查让 AI coding agent 能聚焦修改，也便于人审阅结果。

## 项目结构

| 路径 | 职责 |
| --- | --- |
| `crates/aether-engine/` | 引擎库：渲染器、ECS/场景模型、资源、地形、物理、粒子和时间系统 |
| `crates/aether-launcher/` | 桌面启动器、场景浏览器、编辑器面板和运行时控制 |
| `scenes/` | 示例场景与专项功能验证场景，使用 RON 编写 |
| `tests/` | 自动化检查、视觉测试支持和测试产物 |
| `docs/engine/` | 面向使用者的引擎总览与功能说明 |

## 引擎能做什么

当前渲染器包括延迟 PBR、基于图像的光照、阴影、SSAO、SSR、透明渲染、色调映射、Bloom、地形、水体、大气和体积云。Launcher 提供场景选择、层级与属性面板、变换编辑和模拟控制。

各功能的完成度不同。尤其 SSR 和 SSAO 属于屏幕空间效果，无法获取当前画面之外的信息。功能行为、示例场景和已知边界请查看下方文档。

<table>
  <tr>
    <td width="50%"><img src="docs/images/showcase-materials.png" alt="PBR 材质对比场景" width="100%"></td>
    <td width="50%"><img src="docs/images/showcase-lighting.png" alt="彩色物体与方向光阴影" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>PBR 材质与基于图像的光照</sub></td>
    <td align="center"><sub>彩色材质与方向光阴影</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/showcase-clouds.png" alt="地形上方的体积云" width="100%"></td>
    <td width="50%"><img src="docs/images/showcase-physics-ramp.png" alt="斜坡刚体测试场景" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>地形上方的体积云</sub></td>
    <td align="center"><sub>刚体与斜坡测试场景</sub></td>
  </tr>
</table>

## 文档

- [引擎总览与功能说明](docs/engine/README.md)
- [开发路线图](docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html)
- [工程与验证规范](docs/engineering-governance.md)
- [视觉测试流程](docs/agents/visual-test-workflow.md)

## 运行

安装稳定版 Rust 工具链，并准备 wgpu 支持的图形设备和后端：

```bash
git clone https://github.com/ruochenhua/AetherEngine.git
cd AetherEngine
cargo run -p aether-launcher
```

启动后会打开场景浏览器。也可以直接指定场景：

```bash
cargo run -p aether-launcher -- --scene scenes/24_t3_pbr_material_grid.ron
```

Workspace 在 [`Cargo.toml`](Cargo.toml) 中声明许可证为 `MIT OR Apache-2.0`。
