#!/usr/bin/env bash
# Execute one roadmap slice and preserve its primary-case evidence.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

usage() {
    echo "Usage: $0 --slice T8.1" >&2
}

if [[ $# -ne 2 || "$1" != "--slice" || "$2" != "T8.1" ]]; then
    usage
    exit 2
fi

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    SLICE_PYTHON="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python
else
    echo "Python 3 is required to write the HTML slice report" >&2
    exit 2
fi

RUN_ID="t8-$(date -u +%Y%m%dT%H%M%SZ)-$$"
CASE_ID=t8_asset_store_lifecycle
CASE_FILE=tests/cases/$CASE_ID.json
CASE_DIR=tests/reports/$RUN_ID/$CASE_ID
COMMAND='cargo test -p aether-engine --test t8_asset_store_lifecycle -- --exact t8_asset_store_lifecycle'
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf 'uncommitted')"

mkdir -p "$CASE_DIR"
if command -v cygpath >/dev/null 2>&1; then
    CASE_DIR_NATIVE="$(cygpath -aw "$CASE_DIR")"
    EVENTS_PATH_NATIVE="$(cygpath -aw "$CASE_DIR/asset-events.json")"
else
    CASE_DIR_NATIVE="$CASE_DIR"
    EVENTS_PATH_NATIVE="$CASE_DIR/asset-events.json"
fi

if AETHER_T8_ASSET_EVENTS="$EVENTS_PATH_NATIVE" cargo test -p aether-engine \
    --test t8_asset_store_lifecycle -- --exact t8_asset_store_lifecycle \
    >"$CASE_DIR/stdout" 2>"$CASE_DIR/stderr"; then
    TEST_EXIT=0
else
    TEST_EXIT=$?
fi

REPORT_EXIT=0
"$SLICE_PYTHON" scripts/build_t8_asset_report.py \
    --case-file "$CASE_FILE" \
    --case-dir "$CASE_DIR_NATIVE" \
    --run-id "$RUN_ID" \
    --commit "$COMMIT" \
    --command "$COMMAND" \
    --runner-pid "$$" \
    --started-at "$STARTED_AT" \
    --exit-code "$TEST_EXIT" || REPORT_EXIT=$?

echo "case=$CASE_ID"
echo "report=$CASE_DIR/report.html"
echo "aggregate=$CASE_DIR/aggregate.json"
echo "cargo_test_exit=$TEST_EXIT"

if [[ $TEST_EXIT -ne 0 || $REPORT_EXIT -ne 0 ]]; then
    exit 1
fi
