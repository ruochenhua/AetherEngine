# Scene Load — Open/Import — Task Breakdown

## 任务 2.1：准备工作

- [x] 创建 `openspec/specs/scene-load-open-import/` 目录及 spec 文档
- [x] 验证 `cargo check` 基线通过

## 任务 2.2：重构 build_world 支持相机/灯光/物体分离 spawn

- [ ] 在 `build_world` 中新增相机 entity spawn：`(Transform, Camera)` from `desc.camera`
- [ ] 在 `build_world` 中新增灯光 entity spawn：`(Transform, Light)` from `desc.lights[0]`
- [ ] 在物体 spawn 中附加 `Name` Component：从 `obj.name` 读取
- [ ] 提取 `build_objects()` 私有方法（供 import 复用）

## 任务 2.3：Open/Import 语义实现

- [ ] 新增 `SceneLoader::open_scene(path, device, registry, world)`：
  - 调用 `world.clear()`
  - 调用 `build_world()` 完整加载
- [ ] 修改 `SceneLoader::import_scene()`：
  - 不清空 world
  - 只调用 `build_objects()` 追加物体
  - 返回 `LightingUniforms`（从 RON 构建）

## 任务 2.4：编译验证

- [ ] `cargo check --all-targets` 0 errors
- [ ] `cargo clippy --all-targets` 无新增 warnings

## 任务 2.5：运行时验证

- [ ] 更新现有 loader 测试：验证 `build_world` 后 world 包含相机+灯光+物体
- [ ] 新增 `open_scene_clears_world` 测试
- [ ] 新增 `import_scene_appends_objects` 测试
- [ ] `cargo test -p aether-engine` 全部通过
