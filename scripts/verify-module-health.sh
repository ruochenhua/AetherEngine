#!/usr/bin/env bash
# Aether Engine module-health governance wrapper.
#
# Enforces:
#   - New Rust files must not exceed 500 lines.
#   - Existing files must not grow beyond their recorded baseline.
#
# Usage:
#   ./scripts/verify-module-health.sh            # check
#   ./scripts/verify-module-health.sh --update-baseline

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if python3 -c 'import sys' >/dev/null 2>&1; then
    MODULE_HEALTH_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    MODULE_HEALTH_PYTHON=python
else
    echo "Python 3 is required for module-health verification" >&2
    exit 2
fi

if [[ "${1:-}" == "--update-baseline" ]]; then
    exec "$MODULE_HEALTH_PYTHON" verify_module_health.py --update-baseline
fi

exec "$MODULE_HEALTH_PYTHON" verify_module_health.py --check
