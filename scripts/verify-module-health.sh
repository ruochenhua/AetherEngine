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

if [[ "${1:-}" == "--update-baseline" ]]; then
    exec python3 verify_module_health.py --update-baseline
fi

exec python3 verify_module_health.py --check
