---
name: aether-merge
description: 基于 .aether-changes.yml 的结构化代码合并编排。扫描 git worktree、检测 E/C 层冲突、生成合并计划、逐轮执行合并并验证编译。Use when 需要合并多个 worktree/分支、检测跨模块冲突、编排并行开发成果的集成顺序。
---

# Aether Merge

## 触发条件

- 用户说"合并"、"merge"、"集成"
- 多个 worktree 状态为 `ready-to-merge`
- 视觉测试通过后准备合入 main
- 需要了解当前所有 worktree 的合并状态

## Quick start

```bash
# 查看当前状态
.claude/skills/aether-merge/scripts/aether-merge status

# 生成合并计划
.claude/skills/aether-merge/scripts/aether-merge plan

# 自动合并（按计划逐轮执行）
.claude/skills/aether-merge/scripts/aether-merge execute --auto

# 合并指定分支
.claude/skills/aether-merge/scripts/aether-merge execute --branch feat/phase3-picking

# 清理已合并的 worktree
.claude/skills/aether-merge/scripts/aether-merge clean
```

## 工作流

### Workflow 1: 查看合并全景

```
用户: "当前合并状态如何？"
  │
  ▼
aether-merge status
  │
  ├── 列出所有 worktree: 分支 / Issue / 状态 / 标题
  ├── 显示冲突矩阵 (L0 ~ L3)
  └── 输出建议合并顺序 (Round 1 → Round 2 → ...)
```

### Workflow 2: 生成合并计划

```
aether-merge plan
  │
  ├── 1. git worktree list → 发现所有活跃 worktree
  ├── 2. 读取每个 .aether-changes.yml
  ├── 3. 构建冲突矩阵:
  │     L0: 无重叠 → 可并行合并
  │     L1: 同文件不同函数 → git 自动处理
  │     L2: 签名变更 → 需协调合并顺序
  │     L3: 同符号冲突 → 需人类介入
  ├── 4. 拓扑排序 → 输出合并轮次
  └── 5. 显示依赖链和冲突摘要
```

### Workflow 3: 执行合并

```
aether-merge execute --auto
  │
  ├── For each Round:
  │   ├── 切换到 main
  │   ├── git pull (更新 main)
  │   ├── For each branch in round:
  │   │   ├── git merge --no-ff <branch>
  │   │   ├── 成功?
  │   │   │   ├── cargo build 验证
  │   │   │   ├── 更新 .aether-changes.yml → status: merged
  │   │   │   └── ✅
  │   │   └── 冲突?
  │   │       ├── 显示冲突文件
  │   │       ├── 读取 changes.yml 给上下文
  │   │       └── ⚡ 暂停，等待人类解决
  │   └── 本轮全部成功后继续下一轮
  └── 全部完成
```

### Workflow 4: 合并失败处理

```
合并冲突
  │
  ├── 1. 读取冲突双方的 .aether-changes.yml
  ├── 2. 定位冲突符号（哪个函数/文件）
  ├── 3. 基于 CONTEXT.md 术语给出修复建议
  ├── 4. 输出人类操作指令:
  │     手动解决冲突 → git add . && git commit
  │     或放弃: git merge --abort
  └── 5. 冲突解决后继续 aether-merge execute --auto
```

## 冲突等级参考

详见 [SCHEMA.md](SCHEMA.md) § 冲突等级定义。

| 等级 | 条件 | 自动处理 |
|------|------|----------|
| L0 | entities + components 无重叠 | ✅ 自动合并 |
| L1 | 同文件，不同函数 | ✅ git 自动处理 |
| L2 | A 改签名，B 调该符号 | ⚠️ 需协调顺序 |
| L3 | 同函数体冲突 | ❌ 需人类解决 |

## 与 to-issues 的配合

`to-issues` skill 切 issue 时应生成 .aether-changes.yml 模板：

```yaml
version: 1
issue: "#<NUMBER>"
title: "<TITLE>"
branch: "<BRANCH>"
entities:
  # to-issues 填充初始 scope
components:
  # 开发 agent 在实现过程中更新
depends_on: []
status: in-progress
```

开发 agent 在编码过程中持续更新 `entities` 和 `components`，特别是修改公开符号时必须填写 `signature_before` / `signature_after`。

## 合并后检查清单

- [ ] `cargo build` 通过
- [ ] `cargo test` 通过
- [ ] 运行 aether-visual-test（若涉及渲染 pass）
- [ ] `aether-merge clean` 清理 worktree
- [ ] `git push` 推送 main
