#!/usr/bin/env bash
# Execute one roadmap slice and preserve its primary-case evidence.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

usage() {
    echo "Usage: $0 --slice T8.1|T8.2" >&2
}

if [[ $# -ne 2 || "$1" != "--slice" || ( "$2" != "T8.1" && "$2" != "T8.2" ) ]]; then
    usage
    exit 2
fi

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    SLICE_PYTHON="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python
else
    echo "Python 3 is required to write the HTML slice report" >&2
    exit 2
fi

RUN_ID="t8-$(date -u +%Y%m%dT%H%M%SZ)-$$"
if [[ "$2" == "T8.1" ]]; then
    CASE_ID=t8_asset_store_lifecycle
    TEST_NAME=t8_asset_store_lifecycle
    EVENTS_ENV=AETHER_T8_ASSET_EVENTS
    RENDER_REQUIRED=0
else
    CASE_ID=t8_material_reload_lkg
    TEST_NAME=t8_material_reload_lkg
    EVENTS_ENV=AETHER_T8_MATERIAL_EVENTS
    RENDER_REQUIRED=1
fi
CASE_FILE=tests/cases/$CASE_ID.json
CASE_DIR=tests/reports/$RUN_ID/$CASE_ID
COMMAND="cargo test -p aether-engine --test $TEST_NAME -- --exact $TEST_NAME"
RENDER_COMMAND='not run'
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf 'uncommitted')"

mkdir -p "$CASE_DIR"
if command -v cygpath >/dev/null 2>&1; then
    CASE_DIR_NATIVE="$(cygpath -aw "$CASE_DIR")"
    EVENTS_PATH_NATIVE="$(cygpath -aw "$CASE_DIR/asset-events.json")"
else
    CASE_DIR_NATIVE="$CASE_DIR"
    EVENTS_PATH_NATIVE="$CASE_DIR/asset-events.json"
fi

if env "$EVENTS_ENV=$EVENTS_PATH_NATIVE" cargo test -p aether-engine \
    --test "$TEST_NAME" -- --exact "$TEST_NAME" \
    >"$CASE_DIR/stdout" 2>"$CASE_DIR/stderr"; then
    TEST_EXIT=0
else
    TEST_EXIT=$?
fi

RENDER_EXIT=
if [[ $RENDER_REQUIRED -eq 1 ]]; then
    RENDER_COMMAND='cargo run --release -p aether-launcher -- --scene scenes/t8_material_reload.ron --screenshot <case-dir>/launcher-output.png --exit-after-frames 1 --no-gui-overlay --no-debug-helpers'
    if "$SLICE_PYTHON" scripts/run_t8_material_scene.py --case-dir "$CASE_DIR_NATIVE" \
        >"$CASE_DIR/scene-runner.stdout" 2>"$CASE_DIR/scene-runner.stderr"; then
        RENDER_EXIT=0
    else
        RENDER_EXIT=$?
    fi
fi

REPORT_EXIT=0
REPORT_RENDER_ARGS=()
if [[ -n "$RENDER_EXIT" ]]; then
    REPORT_RENDER_ARGS=(--render-exit-code "$RENDER_EXIT" --render-command "$RENDER_COMMAND")
fi
"$SLICE_PYTHON" scripts/build_t8_asset_report.py \
    --case-file "$CASE_FILE" \
    --case-dir "$CASE_DIR_NATIVE" \
    --run-id "$RUN_ID" \
    --commit "$COMMIT" \
    --command "$COMMAND" \
    --runner-pid "$$" \
    --started-at "$STARTED_AT" \
    --exit-code "$TEST_EXIT" \
    "${REPORT_RENDER_ARGS[@]}" || REPORT_EXIT=$?

echo "case=$CASE_ID"
echo "report=$CASE_DIR/report.html"
echo "aggregate=$CASE_DIR/aggregate.json"
echo "cargo_test_exit=$TEST_EXIT"
if [[ -n "$RENDER_EXIT" ]]; then
    echo "scene_exit=$RENDER_EXIT"
fi

if [[ $TEST_EXIT -ne 0 || $REPORT_EXIT -ne 0 || ( -n "$RENDER_EXIT" && $RENDER_EXIT -ne 0 ) ]]; then
    exit 1
fi
