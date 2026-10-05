#!/usr/bin/env bash
# Exercise the shared T0 report writer and reference-review gates on every OS.

set -euo pipefail

if python3 -c 'import os' >/dev/null 2>&1; then
    REPORT_TEST_PYTHON=python3
elif python -c 'import os' >/dev/null 2>&1; then
    REPORT_TEST_PYTHON=python
else
    echo "Python 3 is required for the report format smoke test" >&2
    exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_ROOT="$(dirname "$SCRIPT_DIR")"
export PYTHONDONTWRITEBYTECODE=1
"$REPORT_TEST_PYTHON" -X utf8 "$SOURCE_ROOT/tests/visual_foundation_test.py" \
    FoundationTests.test_html_machine_status_does_not_hide_degraded \
    FoundationTests.test_pending_reference_and_human_review_are_blocked
