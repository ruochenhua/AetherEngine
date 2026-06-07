# 并行工作流

AetherEngine 使用 git worktree + change manifest + merge skill 三层架构支持多 agent 并行开发。

决策背景见 [ADR-0009: Git Worktree + Change Manifest + Merge Skill](../adr/0009-parallel-work-merge-skill.md)。

## 核心概念

```
一个 worktree = 一个 issue = 一个独立沙盒
     │
     ├── .aether-changes.yml   ← 声明"我碰了什么"
     │     E 层: entities (模块/Pass)
     │     C 层: components (函数/trait)
     │
     └── aether-merge skill    ← 在合并时消费这些声明
           冲突矩阵 → 排序 → 逐轮合并 → 编译验证
```

## 工作流

### 1. 切 Issue（to-issues skill）

```bash
# to-issues 生成 .aether-changes.yml 模板
version: 1
issue: "#47"
title: "Picking: Camera ray + AABB"
branch: "feat/phase3-picking"
entities:
  - path: crates/aether-engine/src/picking/
    kind: add
components: []
depends_on: []
status: in-progress
```

### 2. 创建 Worktree

```bash
git worktree add -b feat/phase3-picking ../feat-phase3-picking main
cd ../feat-phase3-picking
```

### 3. 开发（更新变更声明）

开发 agent 在编码过程中持续更新 `.aether-changes.yml`：

```yaml
components:
  - file: crates/aether-engine/src/renderer/camera.rs
    symbols:
      - name: "FlyCamera::ray_from_screen"
        kind: modified
        signature_before: "fn ray_from_screen(&self, cursor: Vec2) -> Ray"
        signature_after: "fn ray_from_screen(&self, cursor: Vec2, depth: f32) -> Ray"
```

**关键规则**：修改公开符号签名时必须填写 `signature_before` / `signature_after`。

### 4. 标记为就绪

开发完成、视觉测试通过后：

```yaml
status: ready-to-merge
```

### 5. 合并编排（aether-merge skill）

```bash
# 查看全局状态
.claude/skills/aether-merge/scripts/aether-merge status

# 生成合并计划
.claude/skills/aether-merge/scripts/aether-merge plan

# 自动合并
.claude/skills/aether-merge/scripts/aether-merge execute --auto
```

### 6. 清理

```bash
.claude/skills/aether-merge/scripts/aether-merge clean
```

## 分支命名

| 前缀 | 用途 |
|------|------|
| `feat/` | 新功能 |
| `fix/` | Bug 修复 |
| `exp/` | 试验/原型 |
| `refactor/` | 重构 |

## 编译产物共享

`.cargo/config.toml` 已配置：

```toml
[build]
target-dir = "/Users/ruochenhua/.cargo/aether-target"
```

所有 worktree 共享同一份编译产物。**禁止**同时运行 `cargo build`。

## 约束

- 同一时刻不能并发 `cargo build`（共享 target-dir 限制）
- 同一时刻只能一个 worktree 跑视觉测试（GPU 独占）
- 修改公开符号签名时必须更新 `components.symbols[].signature_before/after`
- 改 `Cargo.toml` 后立即提交 `Cargo.lock`
- 定期运行 `aether-merge clean` 防止僵尸 worktree 堆积

## 相关文件

| 文件 | 角色 |
|------|------|
| `.aether-changes.yml` | 变更声明（每个 worktree） |
| `.cargo/config.toml` | 共享 target-dir 配置 |
| `.claude/skills/aether-merge/SKILL.md` | Merge skill 工作流 |
| `.claude/skills/aether-merge/SCHEMA.md` | changes.yml schema 规范 |
| `.claude/skills/aether-merge/scripts/aether-merge` | 合并执行脚本 |
| `docs/adr/0009-parallel-work-merge-skill.md` | 架构决策记录 |
