# .aether-changes.yml Schema

每个 git worktree 根目录下的变更声明文件。由 to-issues skill 生成模板、开发 agent 更新、merge skill 消费。

## 完整 Schema

```yaml
# Schema 版本号
version: 1

# 关联的 GitHub issue
issue: "#47"
title: "Picking: Camera ray + AABB intersection"

# Git 分支名
branch: "feat/phase3-picking"

# E 层：模块/子系统级别变更（粗粒度）
entities:
  - path: crates/aether-engine/src/picking/       # 新增模块
    kind: add
  - path: crates/aether-engine/src/renderer/camera.rs  # 修改
    kind: modify
  - path: crates/aether-launcher/src/main.rs
    kind: modify

# C 层：具体符号级别变更（细粒度，冲突检测核心）
components:
  - file: crates/aether-engine/src/renderer/camera.rs
    symbols:
      - name: "FlyCamera::ray_from_screen"
        kind: modified
        signature_before: "fn ray_from_screen(&self, cursor: Vec2) -> Ray"
        signature_after: "fn ray_from_screen(&self, cursor: Vec2, depth: f32) -> Ray"
      - name: "FlyCamera::project"
        kind: added
  - file: crates/aether-launcher/src/main.rs
    symbols:
      - name: "main"
        kind: modified
        notes: "Register picking system in event loop"

# 依赖：此 worktree 必须在哪些 issue 合并之后才能合并
depends_on: []   # 例：["#45", "#46"]

# 状态
status: in-progress   # in-progress | ready-to-merge | merged

# 合并记录（merge skill 自动填充）
merged:
  at: null            # ISO 时间戳
  by: null            # agent / human
  into: null          # 合并到的目标 branch
```

## 字段说明

### `entities` — 实体层

粗粒度声明，描述"我把哪个模块碰了"。kind 取值：

| kind | 含义 |
|------|------|
| `add` | 新增模块/文件 |
| `modify` | 修改已有模块/文件 |
| `remove` | 删除模块/文件 |

### `components` — 组件层

细粒度声明，描述"我把哪个公开符号改了"。这是冲突检测的核心数据。

#### `symbols[].kind`

| kind | 含义 |
|------|------|
| `added` | 新增公开符号 |
| `modified` | 修改公开符号（改变了签名或行为） |
| `removed` | 删除公开符号 |

#### `symbols[].name` 格式

`TypeName::method_name` 用于方法，`TypeName` 用于独立函数或 trait。

#### `signature_before` / `signature_after`

仅当 `kind: modified` 时需要填写。用 Rust 函数签名格式（不含 body），供 merge skill 判断签名级冲突。

## 最小有效文件

```yaml
version: 1
issue: "#47"
branch: "feat/phase3-picking"
entities: []
components: []
depends_on: []
status: in-progress
```

## 冲突等级定义

merge skill 用以下等级判定两个 changeset 的冲突：

| 等级 | 条件 | 自动处理？ |
|------|------|-----------|
| **L0: 无冲突** | entities 和 components 完全不重叠 | ✅ 自动合并 |
| **L1: 文件重叠，符号不重叠** | 同一个文件，改了不同函数 | ✅ git 自动处理 |
| **L2: 签名变更** | A 改了某符号签名，B 新增了对该符号的调用 | ❌ 需人类确认后自动 rebase |
| **L3: 同符号逻辑冲突** | 两个 worktree 改了同一个函数体 | ❌ 必须人类解决 |
