# Scene Launcher Integration — Task Breakdown

## 任务 4.1：准备工作

- [x] 创建 `openspec/specs/scene-launcher-integration/` 目录及 spec 文档
- [x] 验证 `cargo check` 基线通过

## 任务 4.2：菜单与对话框

- [x] 添加 `Open Scene...` 菜单项（`pending_open_dialog`）
- [x] 保留 `Import Scene...` 菜单项
- [x] `Open Scene` 调用 `SceneLoader::open_scene()`
- [x] `Import Scene` 调用 `SceneLoader::import_scene()`
- [x] Save 对话框自动追加 `.ron` 扩展名

## 任务 4.3：场景加载切换

- [x] CLI `--scene` 改用 `open_scene`（清空 world）
- [x] 菜单场景列表选择改用 `open_scene`（清空 world）
- [x] `New Scene` 保持不变（已清空 world）

## 任务 4.4：Camera ECS 同步

- [x] `read_camera_from_world()` 自由函数：从 ECS 读取相机状态
- [x] `write_camera_to_world()` 自由函数：写回 ECS 相机实体
- [x] 加载后调用 read 初始化 `self.camera`
- [x] 保存前调用 write 同步到 ECS

## 任务 4.5：每帧 Lighting 更新

- [x] 渲染循环中 `match &mut self.state`
- [x] 每帧更新 `lighting.camera_pos` 从 `self.camera.position`
- [x] 每帧更新 `lighting.light` 从 ECS `(Transform, Light)`

## 任务 4.6：编译与运行时验证

- [x] `cargo check --all-targets` 0 errors
- [x] `cargo test --all-targets` 全部通过
- [x] `cargo clippy` 无新增 warnings
