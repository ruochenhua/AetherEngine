# Scene Serializer Complete — Task Breakdown

## 任务 3.1：准备工作

- [x] 创建 `openspec/specs/scene-serializer-complete/` 目录及 spec 文档
- [x] 验证 `cargo check` 基线通过

## 任务 3.2：重构 serialize_world

- [x] 导入 `Light` 和 `Name` 组件
- [x] 提取 `extract_camera` 私有函数：从 `(Transform, Camera)` 读取
- [x] 提取 `extract_lights` 私有函数：从 `(Transform, Light)` 读取所有灯光
- [x] 提取 `extract_objects` 私有函数：从 `(Transform, MeshHandle, MaterialUniform, Visibility, Name)` 读取
- [x] `extract_objects` 读取 `Name` 填充 `ObjectConfig.name`

## 任务 3.3：编译验证

- [x] `cargo check --all-targets` 0 errors
- [x] `cargo clippy --all-targets` 无新增 warnings

## 任务 3.4：运行时验证

- [x] 新增 `extract_camera_roundtrips` 测试
- [x] 新增 `extract_light_roundtrips` 测试
- [x] 新增 `extract_object_preserves_name` 测试
- [x] 新增 `serialize_world_full_roundtrip` 集成测试
- [x] 新增 `serialize_to_ron_roundtrips` 测试
- [x] `cargo test -p aether-engine` 全部通过
