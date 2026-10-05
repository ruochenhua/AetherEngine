#!/usr/bin/env bash
# Run one bounded T1 VisualCase v2 fixture and preserve its acceptance evidence.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if [[ $# -ne 2 || "$1" != "--slice" || ( "$2" != "T1.1" && "$2" != "T1.2" && "$2" != "T1.3" ) ]]; then
    echo "Usage: $0 --slice T1.1|T1.2|T1.3" >&2
    exit 2
fi

if [[ -n "${AETHER_SLICE_PYTHON_BIN:-}" ]]; then
    SLICE_PYTHON="$AETHER_SLICE_PYTHON_BIN"
elif python3 -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python3
elif python -c 'import sys' >/dev/null 2>&1; then
    SLICE_PYTHON=python
else
    echo "Python 3 is required to write the T1 fixture report" >&2
    exit 2
fi

case "$2" in
    T1.1) CASE_ID=t1_editor_transaction; TEST_NAME=t1_editor_transaction ;;
    T1.2) CASE_ID=t1_snapshot_whitelist; TEST_NAME=t1_snapshot_whitelist ;;
    T1.3) CASE_ID=t1_editor_command_replay; TEST_NAME=t1_editor_command_replay ;;
esac

RUN_ID="t1-$(date -u +%Y%m%dT%H%M%SZ)-$$"
CASE_FILE="tests/cases/$CASE_ID.json"
CASE_DIR="tests/reports/$RUN_ID/$CASE_ID"
EVENTS_PATH="$CASE_DIR/editor-events.json"
UI_EVENTS_PATH="$CASE_DIR/editor-ui-events.json"
APPLY_EVENTS_PATH="$CASE_DIR/editor-apply-events.json"
TEST_COMMAND=(cargo test -p aether-engine --test t1_editor_primary "$TEST_NAME" -- --exact)
COMMAND="${TEST_COMMAND[*]}"
UI_EXIT=0
APPLY_EXIT=0
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf 'uncommitted')"

mkdir -p "$CASE_DIR"
if command -v cygpath >/dev/null 2>&1; then
    CASE_DIR_NATIVE="$(cygpath -aw "$CASE_DIR")"
    EVENTS_PATH_NATIVE="$(cygpath -aw "$EVENTS_PATH")"
    UI_EVENTS_PATH_NATIVE="$(cygpath -aw "$UI_EVENTS_PATH")"
    APPLY_EVENTS_PATH_NATIVE="$(cygpath -aw "$APPLY_EVENTS_PATH")"
else
    CASE_DIR_NATIVE="$CASE_DIR"
    EVENTS_PATH_NATIVE="$EVENTS_PATH"
    UI_EVENTS_PATH_NATIVE="$UI_EVENTS_PATH"
    APPLY_EVENTS_PATH_NATIVE="$APPLY_EVENTS_PATH"
fi

if env "AETHER_T1_EDITOR_EVENTS=$EVENTS_PATH_NATIVE" "${TEST_COMMAND[@]}" \
    >"$CASE_DIR/stdout" 2>"$CASE_DIR/stderr"; then
    TEST_EXIT=0
else
    TEST_EXIT=$?
fi

if [[ "$2" == "T1.3" ]]; then
    UI_COMMAND=(cargo test -p aether-launcher app::ui::editor_operations::tests::add_light_button_emits_operation_for_selected_entity -- --exact)
    if env "AETHER_T1_EDITOR_UI_EVENTS=$UI_EVENTS_PATH_NATIVE" "${UI_COMMAND[@]}" \
        >"$CASE_DIR/ui.stdout" 2>"$CASE_DIR/ui.stderr"; then
        UI_EXIT=0
    else
        UI_EXIT=$?
    fi
    if [[ $UI_EXIT -ne 0 ]]; then
        TEST_EXIT=$UI_EXIT
    fi
    COMMAND="$COMMAND && ${UI_COMMAND[*]}"
    APPLY_COMMAND=(cargo test -p aether-launcher app::editor::tests::queued_ui_operation_persists_and_undo_redo_updates_replay_log -- --exact)
    if env "AETHER_T1_EDITOR_APPLY_EVENTS=$APPLY_EVENTS_PATH_NATIVE" "${APPLY_COMMAND[@]}" \
        >"$CASE_DIR/apply.stdout" 2>"$CASE_DIR/apply.stderr"; then
        APPLY_EXIT=0
    else
        APPLY_EXIT=$?
    fi
    if [[ $APPLY_EXIT -ne 0 ]]; then
        TEST_EXIT=$APPLY_EXIT
    fi
    COMMAND="$COMMAND && ${APPLY_COMMAND[*]}"
fi

REPORT_EXIT=0
"$SLICE_PYTHON" scripts/build_t1_editor_report.py \
    --case-file "$CASE_FILE" \
    --case-dir "$CASE_DIR_NATIVE" \
    --slice "$2" \
    --run-id "$RUN_ID" \
    --commit "$COMMIT" \
    --command "$COMMAND" \
    --runner-pid "$$" \
    --started-at "$STARTED_AT" \
    --exit-code "$TEST_EXIT" \
    --ui-exit-code "$UI_EXIT" \
    --apply-exit-code "$APPLY_EXIT" || REPORT_EXIT=$?

echo "slice=$2"
echo "case=$CASE_ID"
echo "report=$CASE_DIR/report.html"
echo "aggregate=$CASE_DIR/aggregate.json"
echo "fixture_exit=$TEST_EXIT"
echo "launcher_started=false"
if [[ "$2" == "T1.3" ]]; then
    echo "ui_event_test_exit=$UI_EXIT"
    echo "operation_apply_test_exit=$APPLY_EXIT"
fi

if [[ $TEST_EXIT -ne 0 || $REPORT_EXIT -ne 0 ]]; then
    exit 1
fi
