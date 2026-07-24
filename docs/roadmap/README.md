# Aether Engine Roadmap

> 本目录是 Aether Engine 的分层实施路线图。实现任务必须按“总览 → 阶段 → 子系统 → 任务”顺序读取；阶段完成前不得跳过其退出条件直接进入后续阶段。

## 阅读顺序

1. `00-current-state.md`：当前基线、已知风险和不变约束。
2. `phase-0-governance/README.md`：工程治理、文档和 AI 协作规则。
3. `phase-1-verification/README.md`：测试、视觉回归和合并门禁。
4. `phase-2-render-graph/README.md`：渲染图资源语义和场景切换正确性。
5. `phase-3-rendering-quality/README.md`：法线、反射、大气和云的质量/性能。
6. `phase-4-runtime-editor/README.md`：资产、运行时状态和编辑器拆分。
7. `phase-5-engine-capabilities/README.md`：地形、透明、动画和流式资源。
8. `phase-6-advanced-evaluation/README.md`：光追、GI 等高级特性的进入条件。

## 依赖关系

```text
Phase 0
  ↓
Phase 1 ───→ Phase 2 ───→ Phase 3
                           ↓
                         Phase 4
                           ↓
                         Phase 5
                           ↓
                         Phase 6
```

## 总体优先级

- P0：可选 Pass 残留输出、视觉回归 fail-open、AI 合并流程、文档漂移。
- P1：Render Graph 版本语义、法线/反射正确性、大气和云的性能。
- P2：资产运行时、编辑器架构、GPU 地形、透明和动画。
- 延后：光追、实时 GI、完整物理和网络同步。

## 全局完成门禁

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p aether-engine --lib
cargo build -p aether-launcher --release
```

涉及渲染或 Launcher 的变更还必须运行云、地形和 SSR 场景，并保存截图及视觉比较结果。任何 Shader 必须加入 `crates/aether-engine/src/renderer/shader_validation.rs` 的 `SHADER_MANIFEST`。
