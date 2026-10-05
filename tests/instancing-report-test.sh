#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(dirname "$0")"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    exec "$AETHER_SLICE_PYTHON_BIN" "$PROJECT_ROOT/scripts/test_t9_instancing_report.py"
elif command -v python3 >/dev/null 2>&1; then
    exec python3 "$PROJECT_ROOT/scripts/test_t9_instancing_report.py"
elif command -v python >/dev/null 2>&1; then
    exec python "$PROJECT_ROOT/scripts/test_t9_instancing_report.py"
fi
echo "Python 3 is required for T9.3 report tests" >&2
exit 2
