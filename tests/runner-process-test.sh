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
if "$RUNNER_TEST_PYTHON" -c 'import os, sys; sys.exit(0 if os.name == "nt" else 1)'; then
    PYTHONDONTWRITEBYTECODE=1 "$RUNNER_TEST_PYTHON" -X utf8 tests/windows_job_test.py
else
    PYTHONDONTWRITEBYTECODE=1 "$RUNNER_TEST_PYTHON" -X utf8 tests/runner_process_test.py
fi
bash -n scripts/verify-regression.sh scripts/verify-regression-metal.sh
./tests/metal-regression-runner-test.sh
