# Aether Engine Visual Tests

This directory holds visual regression tests for Aether Engine scenes.

## Structure

- `reference/` — Golden reference images. Commit these to version control.
- `output/` — Captured screenshots from test runs. Ignored by git.
- `reports/` — Generated test reports. Ignored by git.

## Workflow

1. **Capture** — Run a scene with the launcher CLI:
   ```bash
   cargo run --bin aether-launcher -- \
     --scene scenes/01_deferred.ron \
     --screenshot tests/output/01_deferred.png \
     --exit-after-frames 120 \
     --no-gui-overlay
   ```

2. **Compare** — If a reference exists, compare metrics:
   ```bash
   python3 .claude/skills/aether-visual-test/scripts/compare_images.py \
     tests/reference/01_deferred.png \
     tests/output/01_deferred.png \
     --threshold 0.95
   ```

3. **Inspect** — Agent reads the screenshot and judges quality against the PRD.

4. **Report** — Results are written to `tests/reports/<timestamp>-report.html`.

## Adding a new reference

When a new scene is introduced or a deliberate visual change is accepted:

```bash
# Generate output
cargo run --bin aether-launcher -- \
  --scene scenes/XX_name.ron \
  --screenshot tests/output/XX_name.png \
  --exit-after-frames 120 \
  --no-gui-overlay \
  --width 2560 \
  --height 1440

# Promote to reference
cp tests/output/XX_name.png tests/reference/XX_name.png
git add tests/reference/XX_name.png
```

## Batch regression runner

`tests/visual-matrix.json` defines the canonical visual regression matrix.
Use `scripts/verify-regression.sh` to run the whole matrix or a single scene:

```bash
# Run all scenes in the matrix
./scripts/verify-regression.sh

# Run one scene
./scripts/verify-regression.sh --scene 13_clouds

# Generate or update golden references (only after reviewing the new screenshots)
./scripts/verify-regression.sh --update-references
```

The runner captures deterministic screenshots (`--freeze-time`, `--no-gui-overlay`,
fixed physical size), compares them with `tests/reference/*.png`, writes diff
images, and generates an HTML report under `tests/reports/`. Static debug scenes
such as the SSAO mode 14 cases use one frozen frame; scenes with temporal effects
retain longer frame windows in the matrix.

The runner waits three seconds between successful launcher processes by default.
This is required for reliable Metal teardown on macOS when many scenes are run
back-to-back; override it with `AETHER_REGRESSION_SETTLE_SECONDS=0` only for
diagnostic runs.

For SSR effect verification, use the paired harness so the same scene is captured
once with SSR disabled and once with SSR enabled. It compares the two real outputs
and writes an HTML report; it does not promote either image to a baseline:

```bash
AETHER_LAUNCHER_BIN=target/release/aether-launcher \
  ./tests/ssr-effect-test.sh
```
