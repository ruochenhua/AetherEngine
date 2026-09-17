#!/usr/bin/env bash
# Run the visual regression matrix through one reusable macOS Terminal session.
#
# The renderer still launches one process per scene, but this wrapper creates
# only one Terminal command and waits for it. It also owns cancellation of the
# remote shell tree so Ctrl-C cannot leave a matrix runner behind.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MATRIX_RUNNER="$PROJECT_ROOT/scripts/verify-regression.sh"

# When the command is already running in Terminal, execute the matrix directly
# instead of opening a second session. The same guard prevents recursion from
# the command submitted to Terminal below.
if [[ "${AETHER_METAL_SESSION:-0}" == "1" \
    || "$(uname -s)" != "Darwin" \
    || "${TERM_PROGRAM:-}" == "Apple_Terminal" ]]; then
    exec "$MATRIX_RUNNER" "$@"
fi

RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-metal-regression.XXXXXX")"
PID_FILE="$RUN_DIR/runner.pid"
STATUS_FILE="$RUN_DIR/status"
LOG_FILE="$RUN_DIR/runner.log"

terminate_process_tree() {
    local root="$1"
    [[ "$root" =~ ^[0-9]+$ ]] || return 0

    local child
    while read -r child; do
        [[ -n "$child" ]] || continue
        terminate_process_tree "$child"
    done < <(pgrep -P "$root" 2>/dev/null || true)

    kill -TERM "$root" 2>/dev/null || true
}

cleanup() {
    local exit_code=$?
    if [[ ! -f "$STATUS_FILE" && -f "$PID_FILE" ]]; then
        local runner_pid
        runner_pid="$(<"$PID_FILE")"
        terminate_process_tree "$runner_pid"
    fi
    rm -f "$PID_FILE" "$STATUS_FILE" "$LOG_FILE"
    rmdir "$RUN_DIR" 2>/dev/null || true
    return "$exit_code"
}

trap cleanup EXIT
trap 'exit 130' INT TERM

shell_quote() {
    printf '%q' "$1"
}

LAUNCHER_BIN="${AETHER_LAUNCHER_BIN:-$PROJECT_ROOT/target/release/aether-launcher}"
RUNNER_COMMAND="cd $(shell_quote "$PROJECT_ROOT")"
RUNNER_COMMAND+=" && echo \$\$ > $(shell_quote "$PID_FILE")"
RUNNER_COMMAND+=" && AETHER_METAL_SESSION=1"
if [[ -x "$LAUNCHER_BIN" ]]; then
    RUNNER_COMMAND+=" AETHER_LAUNCHER_BIN=$(shell_quote "$LAUNCHER_BIN")"
fi
RUNNER_COMMAND+=" $(shell_quote "$MATRIX_RUNNER")"
for arg in "$@"; do
    RUNNER_COMMAND+=" $(shell_quote "$arg")"
done
RUNNER_COMMAND+=" > $(shell_quote "$LOG_FILE") 2>&1; rc=\$?; echo \$rc > $(shell_quote "$STATUS_FILE")"

# Use exactly one Terminal command. `reopen` creates a window only when none
# exists; otherwise the command is submitted to the current front window.
APPLE_SCRIPT="$(python3 - "$RUNNER_COMMAND" <<'PY'
import json
import sys

command = sys.argv[1]
print(
    'tell application "Terminal"\n'
    '  if (count of windows) = 0 then reopen\n'
    f'  tell front window to do script {json.dumps(command)}\n'
    'end tell'
)
PY
)"

osascript -e "$APPLE_SCRIPT" >/dev/null

while [[ ! -f "$STATUS_FILE" ]]; do
    sleep 1
done

status="$(<"$STATUS_FILE")"
if [[ "$status" != "0" ]]; then
    echo "Metal regression runner failed (exit $status). Log: $LOG_FILE" >&2
    tail -60 "$LOG_FILE" >&2
    exit "$status"
fi

echo "Metal regression runner completed in one Terminal session."
