#!/usr/bin/env bash
# v2 only: one Python coordinator owns serial, identity-bound launcher leases.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."
if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    PYTHON_BIN="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    PYTHON_BIN=python3
else
    PYTHON_BIN=python
fi
ACTIVE_RUNNER_PID=""
cleanup_active_runner() {
    if [[ -n "$ACTIVE_RUNNER_PID" ]]; then
        kill -TERM "$ACTIVE_RUNNER_PID" 2>/dev/null || true
        wait "$ACTIVE_RUNNER_PID" 2>/dev/null || true
    fi
    ACTIVE_RUNNER_PID=""
}
abort_regression() {
    trap '' INT TERM
    cleanup_active_runner
    exit "$1"
}
trap cleanup_active_runner EXIT
trap 'abort_regression 130' INT
trap 'abort_regression 143' TERM
"$PYTHON_BIN" -X utf8 "$SCRIPT_DIR/run_visual_regression.py" "$@" &
ACTIVE_RUNNER_PID=$!
runner_status=0
wait "$ACTIVE_RUNNER_PID" || runner_status=$?
ACTIVE_RUNNER_PID=""
exit "$runner_status"
