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

## 文件位置约定

示例场景位于 `scenes/`，随场景引用的资源通常位于 `assets/`。移动或重命名文件时，要同步更新 RON 中的相对路径；模型或纹理缺失时，先检查路径和文件大小写是否一致。
