# ADR-0009: Git Worktree + Change Manifest + Merge Skill

## Status

Accepted

## Context

AetherEngine 是 AI-first 项目，多个 AI agent 可能并行处理不同 issue。当前只有一个 `main` branch，没有并行工作的基础设施。

核心挑战：
1. **上下文隔离** — AI agent 在不同任务间切换时，不能受到其他任务半成品代码的干扰
2. **冲突检测** — 多个 agent 改同一模块时，需要在合并前（而非合并时）发现冲突
3. **合并编排** — 合并顺序对冲突解决有决定性影响，需要结构化编排而非随机合并
4. **编译产物共享** — `target/` 已 5.1 GB，每个 worktree 独立编译不可接受

## Decision

采用三层架构：

```
┌─────────────────────────────────────────────────┐
│ Layer 1: git worktree (物理隔离)                 │
│ 一个 worktree = 一个 issue，独立文件系统          │
├─────────────────────────────────────────────────┤
│ Layer 2: .aether-changes.yml (变更声明)          │
│ E 层 (entities: 模块级) + C 层 (components: 符号级)│
├─────────────────────────────────────────────────┤
│ Layer 3: aether-merge skill (合并编排)           │
│ 冲突矩阵 → 拓扑排序 → 逐轮合并 → 编译验证         │
└─────────────────────────────────────────────────┘
```

### Layer 1: git worktree

- 每个 issue 对应一个 worktree，目录名 = branch 名
- Branch 命名：`feat/`、`fix/`、`exp/`、`refactor/`
- 共享编译产物：`.cargo/config.toml` 中 `target-dir` 指向 `/Users/ruochenhua/.cargo/aether-target`
- 约束：同一时刻不能并发 `cargo build`

### Layer 2: .aether-changes.yml

每个 worktree 根目录下放置变更声明文件：

```yaml
version: 1
issue: "#47"
entities:
  - path: crates/aether-engine/src/picking/
    kind: add
components:
  - file: crates/aether-engine/src/renderer/camera.rs
    symbols:
      - name: "FlyCamera::ray_from_screen"
        kind: modified
        signature_before: "fn ray_from_screen(&self, cursor: Vec2) -> Ray"
        signature_after: "fn ray_from_screen(&self, cursor: Vec2, depth: f32) -> Ray"
depends_on: []
status: in-progress
```

E/C 两层映射到项目领域语言：E = 模块/Pass/子系统，C = 公开函数/trait 方法。

### Layer 3: aether-merge skill

```bash
aether-merge plan    # 冲突矩阵 + 合并轮次
aether-merge execute # 逐轮合并 + cargo build 验证
aether-merge status  # 全景视图
aether-merge clean   # 清理已合并 worktree
```

冲突等级：
- **L0**: 无重叠 → 自动合并
- **L1**: 同文件不同函数 → git 自动处理
- **L2**: 签名变更 → 需协调顺序后自动 rebase
- **L3**: 同函数体冲突 → 暂停，需人类解决

### 与 to-issues 的集成

`to-issues` 切 issue 时同时生成 `.aether-changes.yml` 模板（含初始 scope）。
开发 agent 在实现过程中更新 `components`。

## Consequences

### 正面

- **零上下文污染**：每个 agent 的代码物理隔离，不会误读其他 agent 的半成品
- **预知冲突**：合并前就能通过 changes.yml 检测跨模块冲突，而非合并时才炸
- **编译器辅助**：共享 `target/` 使跨 worktree 的增量编译几乎零成本
- **Review 边界清晰**：每个 worktree 的 diff = 一个 PR，人类 reviewer 不会看到混杂的变更
- **失败无副作用**：删 worktree = 完美回滚

### 负面

- **setup 开销**：每个新任务需 `git worktree add` + 创建 changes.yml
- **跨 worktree 无知**：agent 看不到其他 worktree 的代码变更，需依赖 changes.yml
- **并发编译风险**：如果两个 agent 同时 `cargo build` 到同一个 `target-dir`，可能破坏增量编译状态
- **Cargo.lock 竞态**：两个 issue 各自改依赖时，Cargo.lock 必有冲突
- **视觉测试互斥**：wgpu 独占 GPU surface，同一时刻只能一个 worktree 跑视觉测试
- **修复传播延迟**：一个 worktree 发现的 bug 修复需要先 merge 到 main，其他 worktree 才能 rebase

### 缓解措施

- 并发编译：未来可升级到 `sccache` 替代共享 target-dir
- Cargo.lock：改依赖后立即提交 lock file；或用 `.gitattributes` 的 `merge=ours` 策略
- 视觉测试互斥：用 `flock` 做进程级锁（TODO）
- 僵尸 worktree：`aether-merge clean` + 定期 `git worktree prune`
