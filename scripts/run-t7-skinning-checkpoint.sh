#!/usr/bin/env bash
# Produce a bounded review report for the T7.2 CPU/GPU palette core.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    SLICE_PYTHON="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python
else
    echo "Python 3 is required to write the T7.2 checkpoint report" >&2
    exit 2
fi

RUN_ID="t7-core-$(date -u +%Y%m%dT%H%M%SZ)-$$"
CASE_ID=t7_skinned_shadow_reflection
CASE_DIR="tests/reports/$RUN_ID/$CASE_ID"
COMMAND="cargo fmt --all -- --check && cargo test -p aether-engine --lib animation::tests"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf 'uncommitted')"
mkdir -p "$CASE_DIR"

if cargo fmt --all -- --check >"$CASE_DIR/fmt.stdout" 2>"$CASE_DIR/fmt.stderr"; then
    FMT_EXIT=0
else
    FMT_EXIT=$?
fi
if cargo test -p aether-engine --lib animation::tests >"$CASE_DIR/stdout" 2>"$CASE_DIR/stderr"; then
    TEST_EXIT=0
else
    TEST_EXIT=$?
fi
if [[ $FMT_EXIT -ne 0 ]]; then
    TEST_EXIT=$FMT_EXIT
fi

"$SLICE_PYTHON" scripts/build_t7_skinning_checkpoint.py \
    --case-dir "$CASE_DIR" \
    --run-id "$RUN_ID" \
    --commit "$COMMIT" \
    --command "$COMMAND" \
    --runner-pid "$$" \
    --started-at "$STARTED_AT" \
    --format-exit-code "$FMT_EXIT" \
    --test-exit-code "$TEST_EXIT"

echo "case=$CASE_ID"
echo "report=$CASE_DIR/report.html"
echo "aggregate=$CASE_DIR/aggregate.json"
echo "format_exit=$FMT_EXIT"
echo "animation_tests_exit=$TEST_EXIT"
echo "scene_window_started=false"

if [[ $FMT_EXIT -ne 0 || $TEST_EXIT -ne 0 ]]; then
    exit 1
fi
