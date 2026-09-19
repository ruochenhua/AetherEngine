#!/usr/bin/env bash
# Regression checks for the macOS Metal regression runner.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RUNNER="$PROJECT_ROOT/scripts/verify-regression-metal.sh"
REGRESSION_RUNNER="$PROJECT_ROOT/scripts/verify-regression.sh"

test -x "$RUNNER"

# The wrapper must submit one command to the existing Terminal window. A
# second do-script call would create another tab/window during one run.
test "$(rg -c 'do script' "$RUNNER")" -eq 1
rg -q 'tell front window to do script' "$RUNNER"

# Both layers must own cancellation: the wrapper owns the remote shell tree,
# while the matrix runner delegates all launcher ownership to one supervisor.
rg -q 'trap .*cleanup' "$RUNNER"
rg -q 'terminate_process_tree' "$RUNNER"
rg -q 'ACTIVE_RUNNER_PID' "$REGRESSION_RUNNER"
test "$(rg -c 'python3 .*runner_process.py' "$REGRESSION_RUNNER")" -eq 1
! rg -q 'osascript|tell application' "$REGRESSION_RUNNER" "$PROJECT_ROOT/scripts/runner_process.py" "$PROJECT_ROOT/scripts/process_lease.py"
rg -q 'trap .*cleanup' "$REGRESSION_RUNNER"

# The in-terminal handoff must not recursively open another Terminal session.
rg -q 'AETHER_METAL_SESSION' "$RUNNER"
