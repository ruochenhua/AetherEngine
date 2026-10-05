#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    exec "$AETHER_SLICE_PYTHON_BIN" -X utf8 scripts/migrate_visual_cases.py "$@"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    exec python3 -X utf8 scripts/migrate_visual_cases.py "$@"
else
    exec python -X utf8 scripts/migrate_visual_cases.py "$@"
fi
