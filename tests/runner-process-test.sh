#!/usr/bin/env bash
# Headless process-contract tests; never invoke the engine or a GPU.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
PYTHONDONTWRITEBYTECODE=1 python3 tests/runner_process_test.py
bash -n scripts/verify-regression.sh scripts/verify-regression-metal.sh
./tests/metal-regression-runner-test.sh
