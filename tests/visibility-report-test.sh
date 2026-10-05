#!/usr/bin/env bash
# T9.2 paired capture report validation tests.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if python3 -c 'import sys' >/dev/null 2>&1; then
    REPORT_TEST_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    REPORT_TEST_PYTHON=python
else
    echo "Python 3 is required for T9.2 report tests" >&2
    exit 2
fi

"$REPORT_TEST_PYTHON" scripts/test_t9_visibility_report.py
