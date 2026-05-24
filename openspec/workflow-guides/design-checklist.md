# Design 阶段检查清单

> 基于 engine-bootstrap 实践的复盘，每个 change 的 Design 阶段必须逐项检查。

## 1. 依赖兼容性矩阵

- [ ] 列出所有新增/升级/变更的依赖
- [ ] 检查 workspace 中是否已存在同一 crate 的其他版本（`cargo tree -d`）
- [ ] 验证上游依赖的约束（如 egui-wgpu 对 wgpu 的版本锁定）
- [ ] 运行 `cargo check --all-targets` 确认基线编译通过
- [ ] 高风险依赖在 Design 阶段解决，不留到 Apply 阶段

## 2. 影响面分析

- [ ] 使用 grep 统计受影响的文件和引用次数
- [ ] 列出每个受影响文件的具体变更类型（trait 定义、impl 块、导入、调用点）
- [ ] 评估是否需要渐进式迁移策略（新模型 + 兼容层）
- [ ] 如果影响面超过 5 个文件，考虑拆分为多个 change

## 3. 接口签名验证

- [ ] 生命周期约束已标注（如 `'static`）
- [ ] Trait bounds 已明确（如 `Send + Sync + 'static`）
- [ ] 线程安全要求已说明
- [ ] 与现有公共 API 的兼容性已评估

## 4. 验收标准（Acceptance Criteria）

- [ ] 每个 spec 都有明确的 AC
- [ ] AC 包含可自动验证的断言（如 `cargo check` 命令）
- [ ] AC 包含运行时验证标准（如 example 启动无 panic）
- [ ] AC 包含视觉/行为标准（如"窗口标题正确"、"面板显示 FPS"）

## 5. 任务拆分预检

- [ ] 预估编码时间 + 编译修复时间（参考 project-architecture.md 第 10 章）
- [ ] 涉及 trait/生命周期/版本升级时，预留 50% buffer
- [ ] 每个 spec 最后两个任务：编译通过 + 运行时验证
- [ ] 总任务数超过 15 个时，考虑拆分为多个 change

## 6. Spec 依赖关系

- [ ] 每个 spec 声明 `depends_on`
- [ ] 依赖关系无循环
- [ ] 可按拓扑排序串行实现
