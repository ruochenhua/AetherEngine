---
name: aether-visual-test
description: Automated visual regression testing for Aether Engine scenes. Run scenes headlessly, capture screenshots, compare against reference images, and generate structured test reports for TDD validation. Use when verifying Aether Engine rendering features, running scene tests, checking visual regressions, or validating monthly development milestones.
---

# Aether Visual Test

## When to use (MUST trigger)

以下任一条件满足时，**必须**调用本技能：

1. 用户说"验证本月/Phase X 工作"、"跑视觉测试"、"截图验收"
2. Issue 带有 `needs-visual-verify` label
3. PRD 中包含 User Story 涉及视觉效果（IBL/SSAO/SSR/Shadow 等）
4. 修改了 `passes/`、`scheduler.rs`、`lighting.rs`、`context.rs` 或场景加载逻辑
5. 重构后需要确认视觉无回归
6. 新场景首次需要生成参考图

## Quick start

Run a single scene and capture a clean screenshot:

```bash
cargo run --bin aether-launcher -- \
  --scene scenes/01_deferred.ron \
  --screenshot tests/output/01_deferred.png \
  --exit-after-frames 120 \
  --no-gui-overlay
```

Or run the full milestone verification script:

```bash
./scripts/verify-milestone.sh
```

> **Issue-level outputs:** When capturing screenshots as part of an issue or
> feature deliverable, save them to `output/<issue-name>/` instead of
> `tests/output/`. `tests/output/` is reserved for regression-test captures.

## Workflows

### 0. Smart Gate — 判定是否需要视觉验证

**不要无脑跑测试。** 先让 Agent 判断变更范围：

```bash
# 基于最近 commit 的变更自动判定
python3 scripts/should-verify-visual.py --since HEAD~1
```

或基于显式文件列表：

```bash
python3 scripts/should-verify-visual.py \
  --files crates/aether-engine/src/renderer/passes/ssao.rs \
  --issue-text "Add SSAO screen-space ambient occlusion"
```

**判定结果**：

| Exit Code | Verdict | 含义 | 行动 |
|---|---|---|---|
| 0 | **MUST_VERIFY** | 变更涉及 render pass / shader / 场景格式 / 管线编排 | **必须执行步骤 1-4** |
| 1 | **SHOULD_VERIFY** | 变更涉及 renderer 周边（camera、window、依赖版本） | **建议执行步骤 1-4**，时间紧时至少跑关键场景 |
| 2 | **NO_VERIFY** | 纯文档 / 工具 / ECS 逻辑 / 性能优化未动渲染核心 | **跳过视觉测试**，只需 `cargo test` |

**降级规则**：即使 issue 写的是 "Refactor SSAO"，只要文件在 `passes/` 目录下，判定仍是 **MUST_VERIFY**。因为 shader 常量重命名、uniform buffer 重排、循环结构调整都可能无声地改变像素输出。

### 1. Run scene capture

- [ ] Identify the scene file (e.g. `scenes/03_shadow_demo.ron`)
- [ ] Determine expected debug mode / feature flags from PRD or scene purpose
- [ ] Run launcher with CLI args:
  ```bash
  cargo run --bin aether-launcher -- \
    --scene <SCENE_PATH> \
    --screenshot tests/output/<NAME>.png \
    --exit-after-frames 120 \
    --no-gui-overlay \
    [--debug-mode <N>]
  ```
- [ ] Wait for process to exit (auto-exits after specified frames)
- [ ] Verify screenshot exists at output path

**Recommended frame counts by feature:**

| Feature | Frames | Notes |
|---------|--------|-------|
| Basic shading | 60 | Allow 1 sec warm-up |
| SSAO / SSR | 120 | Temporal effects stabilize |
| Shadows | 120 | Shadow map cascade settle |
| IBL | 120 | Environment prefilter ready |

### 2. Reference image comparison (regression detection)

- [ ] Locate reference image at `tests/reference/<NAME>.png`
- [ ] If reference exists, run image comparison:
  ```bash
  python3 .claude/skills/aether-visual-test/scripts/compare_images.py \
    tests/reference/<NAME>.png \
    tests/output/<NAME>.png \
    --threshold 0.95
  ```
- [ ] Record SSIM / MAE / diff% metrics in report
- [ ] If SSIM < threshold, flag as **REGRESSION**
- [ ] If no reference exists, treat as **NEW** — Agent must visually inspect

### 3. Agent visual inspection (quality judgment)

- [ ] Read captured screenshot with `ReadMediaFile`
- [ ] Compare against scene PRD / acceptance criteria:
  - Are expected objects visible and correctly positioned?
  - Are lighting / shadows / SSAO / IBL / SSR effects present?
  - Is the debug mode output correct (if `--debug-mode` was used)?
  - Is the image free of artifacts (black screen, flicker, misalignment)?
- [ ] Judge **PASS** / **FAIL** with specific rationale

### 4. Generate test report

- [ ] Aggregate results from all tested scenes
- [ ] Write `tests/reports/<timestamp>-report.md` with sections:
  - **Summary** — overall PASS / FAIL, scenes tested, regressions found
  - **Per-scene results** — config, screenshot paths, metrics, verdict
  - **Defects** — specific issues for development agent to fix
- [ ] If FAIL, surface the report path to the development agent

## CLI Arguments Reference

| Flag | Description |
|------|-------------|
| `--scene <path>` | Load scene directly, skip menu |
| `--screenshot <path>` | Save PNG screenshot before exit |
| `--exit-after-frames <n>` | Exit after rendering n frames |
| `--no-gui-overlay` | Hide egui overlay in Running state |
| `--debug-mode <n>` | Force debug mode (0-14) |

## Debug mode mapping

| Mode | Name | Typical test purpose |
|------|------|---------------------|
| 0 | Full | Integration / final look |
| 1 | Ambient | Ambient contribution |
| 2 | Diffuse | Diffuse lighting |
| 3 | Specular | Specular highlights |
| 4 | Normals | Normal buffer |
| 5 | NdotL | Lambertian term |
| 6 | Shadow | Shadow map visualization |
| 7 | Direct | Direct lighting only |
| 8 | IBL | Image-based lighting only |
| 9 | Alpha(P) | Alpha (positive) |
| 10 | Alpha(N) | Alpha (negative) |
| 11 | NDC(F2) | NDC space debug |
| 12 | EnvFix(F3) | Environment fix debug |
| 13 | VDir(F4) | View direction debug |
| 14 | SSAO(F5) | SSAO debug |

## Directory Layout

```
tests/
├── reference/          # Golden reference images (commit these)
├── output/             # Regression-test captures (.gitignore)
└── reports/            # Generated test reports (.gitignore)

output/                 # Issue/feature-level screenshots (.gitignore)
└── <issue-name>/
    └── *.png
```

## Tips

- Always use `--no-gui-overlay` for automated screenshots to avoid egui elements in reference images.
- When adding a new scene, generate the first reference manually by running the capture command and copying the output into `tests/reference/`.
- If a deliberate visual change is made, update the reference image after reviewing the new output.
- Run tests from the project root so relative paths (`scenes/`, `tests/`, `assets/`) resolve correctly.
