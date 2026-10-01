#!/usr/bin/env bash
# Headless process-contract tests; never invoke the engine or a GPU.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
if python3 -c 'import os' >/dev/null 2>&1; then
    RUNNER_TEST_PYTHON=python3
elif python -c 'import os' >/dev/null 2>&1; then
    RUNNER_TEST_PYTHON=python
else
    echo "Python 3 is required for the runner process contract tests" >&2
    exit 2
fi
if ! "$RUNNER_TEST_PYTHON" -c 'import os, sys; sys.exit(0 if hasattr(os, "fork") else 1)'; then
    echo "runner process contract tests skipped: process_lease.py requires POSIX process isolation" >&2
    exit 0
fi
PYTHONDONTWRITEBYTECODE=1 "$RUNNER_TEST_PYTHON" tests/runner_process_test.py
bash -n scripts/verify-regression.sh scripts/verify-regression-metal.sh
./tests/metal-regression-runner-test.sh
