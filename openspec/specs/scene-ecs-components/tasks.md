# Scene ECS Components — Task Breakdown

## 任务 1.1：准备工作

- [ ] 创建 `openspec/specs/scene-ecs-components/` 目录及 spec 文档
- [ ] 验证 `cargo check` 基线通过

**验收标准**：
- `cargo check` 0 errors

## 任务 1.2：Light Component 实现

- [ ] 在 `ecs/components.rs` 添加 `Light` struct：
  ```rust
  #[derive(Component, Clone, Debug, Serialize, Deserialize)]
  pub struct Light {
      pub light_type: LightType,
      pub color: [f32; 3],
      pub intensity: f32,
  }
  ```
- [ ] 确认 `LightType` 已 derive `Serialize`/`Deserialize`
- [ ] 添加单元测试：`spawn_light_entity_and_query`

**验收标准**：
- `cargo check` 通过
- 测试：spawn `(Transform, Light)` 后查询返回正确数据

## 任务 1.3：Name Component 实现

- [ ] 在 `ecs/components.rs` 添加 `Name` struct：
  ```rust
  #[derive(Component, Clone, Debug)]
  pub struct Name(pub String);
  ```
- [ ] 添加单元测试：`spawn_named_entity_and_query`

**验收标准**：
- `cargo check` 通过
- 测试：spawn `(Transform, Name)` 后查询返回正确名称

## 任务 1.4：编译验证

- [ ] `cargo check --all-targets` 0 errors
- [ ] `cargo clippy --all-targets` 无新增 warnings
- [ ] 修复所有未使用导入

**验收标准**：
- `cargo check` 0 errors, 0 warnings
- `cargo clippy` 干净

## 任务 1.5：运行时验证

- [ ] `cargo test -p aether-engine` 全部通过
- [ ] 确认新测试覆盖 `Light` 和 `Name` 的 spawn + query

**验收标准**：
- 所有测试绿色
- 无 panic
