#!/usr/bin/env bash
# T8.1 slice runner: lifecycle tests, live evidence, launcher capture and report artifacts.
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"
exec ./scripts/run-slice-tests.sh --slice T8.1 "$@"
