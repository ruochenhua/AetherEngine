# 场景与资源

场景文件描述对象、相机、光照、材质以及可选的环境和模拟配置。Launcher 载入 RON 场景后创建 ECS World；用户编辑并保存场景时，可持久化的数据会重新序列化为 RON。

## RON 场景

`scenes/` 中同时包含演示场景和单一功能的专项验证场景。后者更适合回答具体问题，例如某个材质参数是否生效、阴影是否正确，或水体透明合成是否符合预期。

常见入口：

- PBR 材质：[`24_t3_pbr_material_grid.ron`](../../scenes/24_t3_pbr_material_grid.ron)
- 模型加载：[`19_model_loading_examples.ron`](../../scenes/19_model_loading_examples.ron)
- 水体：[`18_textured_water.ron`](../../scenes/18_textured_water.ron)
- 物理：[`t6_physics_stack.ron`](../../scenes/t6_physics_stack.ron)

可通过命令行打开场景：

```bash
cargo run -p aether-launcher -- --scene scenes/19_model_loading_examples.ron
```

## 资源类型

- **模型网格：**资源加载模块包含 OBJ 与 glTF 加载器；示例见 [`18_file_mesh.ron`](../../scenes/18_file_mesh.ron) 和 [`19_model_loading_examples.ron`](../../scenes/19_model_loading_examples.ron)。
- **纹理与环境图：**纹理、图像缓存和 IBL 相关模块负责读取并管理 GPU 资源；支持格式由当前 image crate 配置和具体加载路径决定。
- **材质资产：**物体可用 `material_asset` 引用独立的 MaterialAsset RON 文件。路径相对于项目根目录；设置后该文件中的材质覆盖场景内联的 `material`。示例见 [`t8_material_reload.ron`](../../scenes/t8_material_reload.ron) 和 [`t8_material_reload.ron` 材质文件](../../assets/materials/t8_material_reload.ron)。
- **程序化地形：**地形几何、LOD 与材质由 `terrain` 模块负责，场景配置控制地形实例及其参数。

场景物体配置示例：

```ron
(
    name: "LinkedMaterial",
    mesh: Builtin("sphere"),
    transform: (translation: (0.0, 0.0, 0.0), scale: (1.0, 1.0, 1.0)),
    material_asset: Some("assets/materials/example.ron"),
)
```

Launcher 运行时会监视场景所引用的材质文件。文件变化后，资产在后台重新加载，并在帧边界应用；解析或依赖加载失败时保留上一个可用材质，并在 Inspector 中显示诊断。选择 **Detach and edit** 会解除文件链接，再把当前材质作为场景内联值编辑；该操作支持撤销和重做。

Launcher 也会监视已引用 MaterialAsset 的纹理依赖和场景中的 Prefab 文件。纹理的新 CPU 代次在帧边界提交到 GPU 缓存后，依赖材质会重新解析并切换到新代次；Prefab 会先在临时实例中完成校验、依赖解析和实体创建，再替换场景里的旧实例。加载失败时会保留当前可用代次或旧 Prefab 实例；材质和 Prefab 诊断会显示在 Inspector。应用关闭时会停止接收资源任务，并在配置的等待时间内 join 共享 worker；超时后可在 worker 完成时重试关闭。

T8.4 验收用例为 `t8_hot_reload_shutdown`，运行 `./scripts/run-slice-tests.sh --slice T8.4` 会生成 HTML/JSON 报告，并把渲染场景限制为一帧后自动退出。T8.3 和 T8.4 的完整运行报告及截图分别归档在 [`t8_prefab_roundtrip`](../../tests/reports/t8-20261001T083138Z-1923/t8_prefab_roundtrip/report.html) 和 [`t8_hot_reload_shutdown`](../../tests/reports/t8-20261001T091626Z-163/t8_hot_reload_shutdown/report.html)。两次 Launcher 场景都以退出码 0 结束；截图已留存，但因没有批准的 golden reference，图片差异比较未运行。全部 T8 slice 的运行记录见 [T8 执行包归档](../plans/2026-09-17-non-raytracing-engine-roadmap/t8-assets.html)。

## Prefab

Prefab 资产使用版本化 RON 文件保存节点树、稳定节点 id 和闭合组件值。每个节点必须有唯一的 `Transform` 组件记录；运行时创建全新的 ECS entity，并在场景序列化时保留 Prefab 路径和实例 override。v1 文件中的 `node.transform` 会迁移到组件记录；重复的旧字段与 `Transform` 记录会报错。

场景通过 `prefab_instances` 放置 Prefab。实例 id 在同一场景中必须唯一，资源路径相对项目根目录。实例 patch 按 `instance_id` 指定节点和字段，patch 覆盖 Prefab 默认值；`removed_components` 用显式 tombstone 移除该实例上的组件。Transform 不能移除，重复 patch、缺失节点或不存在的组件会在写入 ECS 前拒绝，依赖解析失败也不会留下半个实例。

示例见 [`t8_prefab_roundtrip.ron` 场景](../../scenes/t8_prefab_roundtrip.ron) 和 [`t8_prefab_roundtrip.ron` Prefab 资产](../../assets/prefabs/t8_prefab_roundtrip.ron)。当前闭合组件范围复用编辑器的 Transform/Mesh/Material/Visibility/Name/Light/Camera/Atmosphere/Clouds 记录；带多个材质子网格的文件 Mesh 暂不支持 Prefab 实例化。

## 文件位置约定

示例场景位于 `scenes/`，随场景引用的资源通常位于 `assets/`。移动或重命名文件时，要同步更新 RON 中的相对路径；模型或纹理缺失时，先检查路径和文件大小写是否一致。
