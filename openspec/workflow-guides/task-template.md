# Task 拆分标准模板

> 基于 engine-bootstrap 实践的 task 模板，每个 spec 的 task 必须遵循此结构。

## 标准 Task 结构

每个 spec 的 task 列表必须包含以下五类任务：

### 1. 准备工作（Setup）

```markdown
## 任务 X.1：[Spec 名称] 准备工作

- [ ] 创建/修改必要的文件和目录结构
- [ ] 添加/更新 Cargo.toml 依赖
- [ ] 验证 `cargo check` 基线通过

**验收标准**：
- `cargo check` 0 errors（允许 doc warnings）
```

### 2. 核心实现（Implementation）

```markdown
## 任务 X.2：[功能点] 实现

- [ ] 实现 [具体功能]
- [ ] 添加单元测试（如适用）

**验收标准**：
- 代码通过 `cargo check`
- 核心逻辑有基本测试覆盖
```

### 3. 集成（Integration）

```markdown
## 任务 X.3：[功能点] 集成到主流程

- [ ] 在 App/Example 中接入新功能
- [ ] 验证与其他模块的交互正常

**验收标准**：
- Example 编译通过
- 无运行时 panic
```

### 4. 编译验证（Compile Check）【强制】

```markdown
## 任务 X.4：编译验证

- [ ] `cargo check` 0 errors, 0 warnings（示例级别）
- [ ] `cargo clippy` 无严重警告
- [ ] 修复所有未使用导入、死代码

**验收标准**：
- `cargo check --example <example_name>` 0 errors
- `cargo check --lib -p aether-engine` 无新增 warnings
```

### 5. 运行时验证（Runtime Verification）【强制】

```markdown
## 任务 X.5：运行时验证

- [ ] `cargo run --example <example_name>` 成功启动
- [ ] 无 panic、无 wgpu validation error
- [ ] 视觉/行为符合预期

**验收标准**：
- 程序启动后 5 秒内无错误日志
- 窗口标题、UI 文本、渲染结果符合 spec
```

---

## Buffer 预留规则

| 变更类型 | 核心实现 task 数 | 必须预留的 buffer task |
|---------|----------------|---------------------|
| 新增独立模块 | 2-3 | 编译验证 + 运行时验证 |
| 接口签名调整 | 2 | 编译验证 + 运行时验证 + 1 个修复 buffer |
| 依赖版本升级 | 1-2 | 编译验证 + 运行时验证 + 2 个修复 buffer |
| ECS trait 迁移 | 3-4 | 编译验证 + 运行时验证 + 2 个修复 buffer |
| 跨模块重构 | 4-6 | 编译验证 + 运行时验证 + 3 个修复 buffer |

**规则**：如果编译错误超过 20 个，暂停编码，回退到 Design 阶段重新评估。

---

## 示例：wgpu-context-init 的 Task 拆分

```markdown
## 任务 2.1：wgpu Instance 和 Surface 初始化

- [ ] 创建 `RenderContext` 结构体
- [ ] 实现 `Instance::new()` 和 `create_surface()`
- [ ] 处理 Surface 的 `'static` 生命周期（`Arc<Window>`）

**验收标准**：
- `cargo check` 通过
- `RenderContext::new()` 编译通过

## 任务 2.2：Adapter 和 Device 请求

- [ ] 实现 `request_adapter()` 和 `request_device()`
- [ ] 添加 GPU 信息日志输出
- [ ] 配置 Surface 格式和呈现模式

**验收标准**：
- `cargo check` 通过
- 能正确获取 adapter info

## 任务 2.3：编译验证

- [ ] `cargo check --example 01_triangle` 0 errors
- [ ] 修复所有 warnings

## 任务 2.4：运行时验证

- [ ] `cargo run --example 01_triangle` 启动无 panic
- [ ] 验证 wgpu 初始化日志输出
```
