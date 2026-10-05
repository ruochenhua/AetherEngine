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

## T9 performance slices

- `./scripts/run-slice-tests.sh --slice T9.1` runs the bounded baseline capture.
- `./scripts/run-slice-tests.sh --slice T9.2` runs 1k/10k visibility and LOD captures. Both launcher windows run hidden offscreen and exit after 150 frames.
- `./scripts/run-slice-tests.sh --slice T9.3` captures the same 10k fixture with opaque instancing off/on, validates visual/state equivalence and the 30% draw-call gate, then writes a paired HTML report. Each launcher capture exits after 150 frames.
- `./scripts/run-slice-tests.sh --slice T9.4` runs an explicit opt-in, offscreen wgpu occlusion prototype (30 warm-up + 120 GPU-timestamp samples where supported), verifies its visibility mask against a CPU reference, and writes a streaming-memory model plus RFC/report. It never starts the launcher or opens a window. GPU timestamps unavailable on the adapter remain `null`; that yields a production no-go instead of fabricated timings.
- T9.3 acceptance contract and current status: `docs/plans/2026-09-17-non-raytracing-engine-roadmap/t9.3-execution.md`.
- T9.4 research contract and current status: `docs/plans/2026-09-17-non-raytracing-engine-roadmap/t9.4-execution.md`.
- `bash tests/benchmark-report-test.sh` and `bash tests/visibility-report-test.sh` run the focused report contract tests.
- `python scripts/test_t9_research_report.py` checks T9.4 report and unavailable-metric handling without a GPU or launcher.

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

# Promote to reference after reviewing the HTML report
./scripts/verify-regression.sh --scene XX_name \
  --update-references --reason "approved new scene baseline"
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
./scripts/verify-regression.sh --update-references --reason "approved baseline refresh"
```

The runner captures deterministic screenshots (`--freeze-time` or an explicit
`--simulation-time`, always with `--no-gui-overlay` and fixed physical size),
compares them with `tests/reference/*.png`, writes diff images, and generates an
HTML report under `tests/reports/`. Static debug scenes such as the SSAO mode 14
cases use one frozen frame; scenes with temporal effects retain longer frame
windows in the matrix. When `simulation_time` is present in a matrix default or
scene, the runner passes `--time-mode seek --simulation-time <seconds>` so
repeated captures use the same deterministic sample.

Reference promotion is deliberately separate from comparison. It requires both
`--update-references` and a non-empty `--reason TEXT`; the reason is recorded in
the HTML report. A launcher, screenshot, or comparator failure never updates a
reference.

The runner waits three seconds between successful launcher processes by default.
This is required for reliable Metal teardown on macOS when many scenes are run
back-to-back; override it with `AETHER_REGRESSION_SETTLE_SECONDS=0` only for
diagnostic runs.

The shell starts one standard-library Python coordinator for the selected v1
matrix. It executes one launcher at a time, then compares the captures using
the existing thresholds and HTML statuses. Set `AETHER_LAUNCHER_BIN` to use an
existing executable; otherwise the shell builds the debug launcher once.
Each case keeps `stdout`, `stderr`, and JSON `launcher.log` in
`tests/reports/<run-id>/<case-id>/`. The log includes argv, nonce, OS process
identity, state transitions, timing, signals, exit status, and CPU/OS metadata;
the coordinator never queries a GPU. A prior screenshot is retained as
`previous-output.png` in that directory before a new capture is attempted.

The launcher writes one byte to an inherited `AETHER_READY_FD` after its first
successful frame presentation (or submission for a screenshot-only frame).
Without that environment variable, launcher behavior is unchanged. Production
timeouts are 2 seconds for readiness, 120 seconds **from readiness** for a case,
and 5 seconds between TERM and KILL. Override them with
`AETHER_HANDSHAKE_TIMEOUT`, `AETHER_CASE_TIMEOUT`, and
`AETHER_TERMINATION_GRACE`, or the coordinator's `--handshake-timeout`,
`--case-timeout`, and `--grace-period` options.

## T7.1 glTF document and skeleton acceptance

Run the owned-document fixture with:

```bash
./scripts/run-slice-tests.sh --slice T7.1
```

The CPU-only library and fixture tests check a 128-joint skin, cyclic node
rejection, weight and inverse-bind validation, typed animation channels, and a
129-joint primitive's static fallback while retaining another mesh item. They do not start the
launcher or open a scene window. Each run writes `report.html`,
`gltf-diagnostics.json`, `aggregate.json`, logs, metrics, and deterministic PNG
sentinels under `tests/reports/<run-id>/t7_gltf_document_fixture/`.

## T7.2/T7.3 glTF skinning and playback review

Build the release launcher and capture the dedicated glTF actor:

```powershell
cargo build -p aether-launcher --release
python scripts/run_t7_skinning_visual.py
```

The runner loads `tests/fixtures/t7_animated_actor.gltf`, samples its
`RaiseArms` clip, extracts a GPU palette, and captures bind and animated poses
through GBuffer, Shadow, and WaterReflection. It renders one frame and exits;
on Windows the test window remains off-screen. The report includes the image,
launcher logs, command, aggregate data, and SHA-256 pose states at 0, 0.25, 0.5,
and 0.75 seconds. The current manual-review artifact is under
`tests/reports/t7-manual-review-20261003/t7_gltf_animation_playback/`.

This acceptance actor exercises the typed document and animation pipeline.
General scene-config spawning and editor playback controls remain separate
follow-up work.

To inspect live playback, start the release launcher with
`--scene scenes/t7_skinned_shadow_reflection.ron --animation on` in its default
wall-clock mode and leave the GUI overlay enabled. The Inspector's Animation
section provides Play/Pause, time seek, speed, and loop controls. Deterministic
`seek`/`freeze` captures hold the clip endpoint so the bind and animated poses
remain directly comparable.

For explicit case lists, invoke `python3 scripts/runner_process.py --launcher
<binary> --cases <json-file> --report-dir tests/reports/<unique-run-id>`. The
JSON list contains `{ "id": "case-id", "scene": "scene-path", "launcher_args":
["--freeze-time"] }` objects. Repeat `--case <id>` to select entries; execution
always follows manifest order. This is a coordinator input adapter, not a
VisualCase schema migration.

Process isolation currently supports macOS and Linux with `/bin/ps` and
`/dev/fd`; other platforms fail before launching. The direct child stays
unreaped through group signalling so its PID/PGID cannot be reused. Descendants
must remain in the group (daemonizing with `setsid` is outside this contract).
If group identity cannot be verified or the group cannot be emptied, the run
stops without launching another case. SIGINT, SIGTERM, and normal/error exits
clean up; SIGKILL cannot be intercepted.

Run `./tests/runner-process-test.sh` for bounded fake-process tests and static
wrapper checks, with no engine, window, or GPU. It is included in `verify-ci.sh`.

On macOS, use the single-session wrapper when Metal is required from an agent
or another non-Terminal shell:

```bash
./scripts/verify-regression-metal.sh
```

It submits one command to the existing front Terminal window, runs all matrix
scenes serially there, and cleans up the active launcher when interrupted. If
it is already invoked from Terminal, it runs the matrix in that same session
without opening another window. The wrapper is the only supported entry point
for agent-driven Metal regression runs.

For SSR effect verification, use the paired harness so the same scene is captured
once with SSR disabled and once with SSR enabled. It compares the two real outputs
and writes an HTML report; it does not promote either image to a baseline:

```bash
AETHER_LAUNCHER_BIN=target/release/aether-launcher \
  ./tests/ssr-effect-test.sh
```

## T8.1 AssetStore lifecycle acceptance

Run the T8.1 primary fixture with:

```bash
./scripts/run-slice-tests.sh --slice T8.1
```

This is a headless CPU fixture; it does not start the launcher or open a scene
window. Each run writes an HTML aggregate and machine-readable evidence under
`tests/reports/<run-id>/t8_asset_store_lifecycle/`, including the lifecycle
events, probe results, logs, metrics, graph placeholder, and 1×1 RGBA8 sentinel
images. Visual comparison is marked not applicable for this fixture.
