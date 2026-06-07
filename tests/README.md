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

4. **Report** — Results are written to `tests/reports/<timestamp>-report.md`.

## Adding a new reference

When a new scene is introduced or a deliberate visual change is accepted:

```bash
# Generate output
cargo run --bin aether-launcher -- \
  --scene scenes/XX_name.ron \
  --screenshot tests/output/XX_name.png \
  --exit-after-frames 120 \
  --no-gui-overlay

# Promote to reference
cp tests/output/XX_name.png tests/reference/XX_name.png
git add tests/reference/XX_name.png
```
