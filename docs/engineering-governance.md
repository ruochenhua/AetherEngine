# Aether Engine 工程治理规则

本文档是模块健康度与代码质量的**强制性规则**。所有代码变更都必须遵守，
CI 会尽可能自动检查；无法自动检查的规则由 Code Review / Agent 自检强制。

## 1. 自动化强制规则

以下规则已接入 `scripts/verify-ci.sh`，任何 PR / 提交前必须全部通过：

| 规则 | 命令 | 失败时 |
|------|------|--------|
| 代码格式统一 | `cargo fmt --check` | 必须先运行 `cargo fmt` |
| Clippy 零警告 | `cargo clippy --workspace --all-targets -- -D warnings` | 必须修复，不允许 `allow` 绕过 |
| 全量测试通过 | `cargo test --workspace` | 必须修复 |
| Release 可构建 | `cargo build --workspace --release` | 必须修复 |
| 模块健康度 | `./scripts/verify-module-health.sh` | 见下方模块规则 |

## 2. 模块健康度规则

### 2.1 行数上限

- 任何新增 Rust 文件，行数 **不得超过 500 行**。
- 现有超过 500 行的文件记录在 `scripts/module-health-baseline.json` 中。
- 已有超限文件**只允许变小，不允许继续变大**。
- 如果要在超限文件中增加代码，必须先拆分/重构，使其行数降到 500 以下或至少不增加。

### 2.2 检查方式

```bash
# 检查当前是否违反规则
./scripts/verify-module-health.sh

# 只有在完成一次有意的重构、且确认新基线合理后，才允许更新基线
./scripts/verify-module-health.sh --update-baseline
```

> `--update-baseline` 不能作为普通开发流程的一部分随意使用；
> 它只应该在“拆完大文件并经过完整验证”后，用来记录新的更健康基线。

### 2.3 当前超限文件

当前基线中的超限文件是明确的“治理目标”，应按优先级逐个拆分：

- ~~`renderer/ibl/generate.rs`~~（已拆分为 `brdf` / `cubemap` / `equirect` / `hdr` / `irradiance` / `prefilter` / `shaders`）
- ~~`renderer/passes/water_reflection.rs`~~（已拆分为 `water_reflection/pass` / `pipeline` / `shaders` / `terrain`）
- ~~`renderer/passes/atmosphere.rs`~~（已拆分为 `atmosphere/pass` / `pipeline` / `shaders`）
- ~~`renderer/passes/god_ray.rs`~~（已拆分为 `god_ray/pass` / `pipeline` / `shaders` / `tests`）
- ~~`renderer/passes/ssr/pipeline.rs`~~（已拆分为 `ssr/pipeline/trace_shader` / `upsample_shader`）
- ~~`scene/config/mod.rs`~~（已拆分为 `config/mod` + `config/tests`）
- ~~`renderer/passes/shadow.rs`~~（已拆分为 `shadow` + `shaders` + `tests`）
- ~~`renderer/passes/lighting/pipeline.rs`~~（已拆分为 `lighting/pipeline` + `shaders`）
- ~~`renderer/passes/gbuffer.rs`~~（已拆分为 `gbuffer` + `shaders` + `tests`）
- ~~`scene/loader/mod.rs`~~（已拆分为 `loader/mod` + `loader/tests`）
- ~~`crates/aether-launcher/src/app.rs`~~（已拆分为 `app` + `app/handler`）
- ~~`renderer/passes/ssao.rs`~~（已拆分为 `ssao` + `shaders` + `tests`）
- ~~`renderer/passes/fxaa.rs`~~（已拆分为 `fxaa` + `shaders` + `tests`）
- ~~`scene/serializer.rs`~~（已拆分为 `serializer` + `tests`）
- ~~`renderer/passes/water/pipeline.rs`~~（已拆分为 `water/pipeline` + `shaders`）
- ~~`renderer/gizmo.rs`~~（已拆分为 `gizmo` + `math`）
- 其余见 `scripts/module-health-baseline.json`

## 3. 重构纪律

### 3.1 一次只做一件事

- 一个提交只做一种变更：纯重构 / 功能开发 / 文档更新 / 依赖升级。
- 禁止在拆分文件的同时修改渲染行为、新增功能或调整算法。

### 3.2 重构必须验证

涉及渲染 Pass / Shader / 管线 / 场景格式的重构，除 CI 外还必须执行：

```bash
./scripts/verify-regression.sh --scene <受影响的场景>
```

如果没有合适场景，至少运行 `./scripts/verify-regression.sh` 中的相关子集，
并人工/Agent 检查 diff 图。

### 3.3 更新基线的前提

只有在以下条件全部满足时，才允许运行 `--update-baseline`：

1. 重构已完成；
2. `cargo test --workspace` 通过；
3. `cargo clippy --workspace --all-targets -- -D warnings` 通过；
4. 受影响的视觉回归场景通过；
5. 新的行数基线确实比旧基线更健康（总超限文件数或超限行数下降）。

## 4. 代码质量红线

- 不允许新增 `unwrap()` / `expect()` 到生产代码路径（测试代码可豁免）。
- 不允许为了通过 Clippy 而随意 `#[allow]`；确有必要时必须写注释说明原因。
- 不允许把多个模块塞进同一个文件来规避行数检查。
- 不允许删除或绕过 `verify-ci.sh` 中的检查步骤。

## 5. 提交前 Checklist

```text
[ ] cargo fmt --check
[ ] cargo clippy --workspace --all-targets -- -D warnings
[ ] cargo test --workspace
[ ] cargo build --workspace --release
[ ] ./scripts/verify-module-health.sh
[ ] 如涉及渲染：./scripts/verify-regression.sh --scene <相关场景>
[ ] 如更新基线：已确认新基线更健康且经过完整验证
```
