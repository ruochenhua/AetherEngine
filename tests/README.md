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
