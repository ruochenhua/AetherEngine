#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

SLICE=""
MANIFEST=""
LAUNCHER="${AETHER_LAUNCHER_BIN:-}"
REPORT_DIR=""
CASE_TIMEOUT="${AETHER_CASE_TIMEOUT:-120}"
GRACE_PERIOD="${AETHER_TERMINATION_GRACE:-5}"
HANDSHAKE_TIMEOUT="${AETHER_HANDSHAKE_TIMEOUT:-30}"
COMMAND_JSON=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --slice) SLICE="${2:?missing value for --slice}"; shift 2 ;;
        --manifest) MANIFEST="${2:?missing value for --manifest}"; shift 2 ;;
        --launcher) LAUNCHER="${2:?missing value for --launcher}"; shift 2 ;;
        --report-dir) REPORT_DIR="${2:?missing value for --report-dir}"; shift 2 ;;
        --case-timeout) CASE_TIMEOUT="${2:?missing value for --case-timeout}"; shift 2 ;;
        --grace-period) GRACE_PERIOD="${2:?missing value for --grace-period}"; shift 2 ;;
        --handshake-timeout) HANDSHAKE_TIMEOUT="${2:?missing value for --handshake-timeout}"; shift 2 ;;
        --command-json) COMMAND_JSON="${2:?missing JSON argv for --command-json}"; shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$SLICE" ]] || { echo "--slice is required" >&2; exit 2; }
[[ -n "$MANIFEST" ]] || MANIFEST="tests/slices/${SLICE}.json"
[[ -f "$MANIFEST" ]] || { echo "Missing VisualCase v2 manifest: $MANIFEST" >&2; exit 2; }
if [[ -z "$LAUNCHER" ]]; then
    cargo build --bin aether-launcher --quiet
    TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    LAUNCHER="$TARGET_DIR/debug/aether-launcher"
fi
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-${SLICE//./-}-$$"
REPORT_DIR="${REPORT_DIR:-tests/reports/$RUN_ID}"
TEMP_MANIFEST="$(mktemp "${TMPDIR:-/tmp}/aether-v2.XXXXXX.json")"
trap 'rm -f "$TEMP_MANIFEST"' EXIT
cargo run --quiet -p aether-engine --bin visual-case-materialize -- --root "$PROJECT_ROOT" --manifest "$MANIFEST" > "$TEMP_MANIFEST"
RUNNER_ARGS=(--slice "$SLICE" --materialized "$TEMP_MANIFEST" --launcher "$LAUNCHER"
    --report-dir "$REPORT_DIR" --case-timeout "$CASE_TIMEOUT" --handshake-timeout "$HANDSHAKE_TIMEOUT"
    --grace-period "$GRACE_PERIOD")
if [[ -n "$COMMAND_JSON" ]]; then RUNNER_ARGS+=(--command-json "$COMMAND_JSON"); fi
python3 "$SCRIPT_DIR/run_visual_case_v2.py" "${RUNNER_ARGS[@]}"
