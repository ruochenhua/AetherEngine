#!/usr/bin/env bash
# Run the bounded CPU-only T7.1 acceptance fixture and preserve its evidence.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if [[ $# -ne 2 || "$1" != "--slice" || "$2" != "T7.1" ]]; then
    echo "Usage: $0 --slice T7.1" >&2
    exit 2
fi

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    SLICE_PYTHON="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python
else
    echo "Python 3 is required to write the T7.1 HTML report" >&2
    exit 2
fi

RUN_ID="t7-$(date -u +%Y%m%dT%H%M%SZ)-$$"
CASE_ID=t7_gltf_document_fixture
CASE_FILE="tests/cases/$CASE_ID.json"
CASE_DIR="tests/reports/$RUN_ID/$CASE_ID"
EVENTS_PATH="$CASE_DIR/gltf-diagnostics.json"
UNIT_COMMAND=(cargo test -p aether-engine --lib)
FIXTURE_COMMAND=(cargo test -p aether-engine --test "$CASE_ID" -- --exact "$CASE_ID")
COMMAND="${UNIT_COMMAND[*]} && ${FIXTURE_COMMAND[*]}"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf 'uncommitted')"

mkdir -p "$CASE_DIR"
if command -v cygpath >/dev/null 2>&1; then
    CASE_DIR_NATIVE="$(cygpath -aw "$CASE_DIR")"
    EVENTS_PATH_NATIVE="$(cygpath -aw "$EVENTS_PATH")"
else
    CASE_DIR_NATIVE="$CASE_DIR"
    EVENTS_PATH_NATIVE="$EVENTS_PATH"
fi

if "${UNIT_COMMAND[@]}" >"$CASE_DIR/unit.stdout" 2>"$CASE_DIR/unit.stderr"; then
    UNIT_EXIT=0
else
    UNIT_EXIT=$?
fi

if env "AETHER_T7_GLTF_EVENTS=$EVENTS_PATH_NATIVE" "${FIXTURE_COMMAND[@]}" \
    >"$CASE_DIR/fixture.stdout" 2>"$CASE_DIR/fixture.stderr"; then
    FIXTURE_EXIT=0
else
    FIXTURE_EXIT=$?
fi
TEST_EXIT=0
if [[ $UNIT_EXIT -ne 0 ]]; then
    TEST_EXIT=$UNIT_EXIT
elif [[ $FIXTURE_EXIT -ne 0 ]]; then
    TEST_EXIT=$FIXTURE_EXIT
fi
cat "$CASE_DIR/unit.stdout" "$CASE_DIR/fixture.stdout" >"$CASE_DIR/stdout"
cat "$CASE_DIR/unit.stderr" "$CASE_DIR/fixture.stderr" >"$CASE_DIR/stderr"

REPORT_EXIT=0
"$SLICE_PYTHON" scripts/build_t7_gltf_report.py \
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
echo "fixture_exit=$TEST_EXIT"
echo "launcher_started=false"

if [[ $TEST_EXIT -ne 0 || $REPORT_EXIT -ne 0 ]]; then
    exit 1
fi
