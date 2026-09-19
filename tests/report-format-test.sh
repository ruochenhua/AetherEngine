#!/usr/bin/env bash
# Smoke-test that report-producing verification scripts emit HTML.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_ROOT="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(mktemp -d)"
mkdir -p "$PROJECT_ROOT/scripts" "$PROJECT_ROOT/tests/reference"
cp "$SOURCE_ROOT/scripts/verify-regression.sh" "$SOURCE_ROOT/scripts/verify-milestone.sh" \
    "$SOURCE_ROOT/scripts/runner_process.py" "$SOURCE_ROOT/scripts/process_lease.py" "$PROJECT_ROOT/scripts/"
export PYTHONDONTWRITEBYTECODE=1
export AETHER_REGRESSION_SETTLE_SECONDS=0
REPORT_DIR="$PROJECT_ROOT/tests/reports"
TEMP_BIN="$(mktemp -d)"
REGRESSION_NAME="report-format-regression-$$"
MILESTONE_NAME="report-format-milestone-$$"
COMPARE_ERROR_NAME="report-format-compare-error-$$"
REGRESSION_REPORT="$REPORT_DIR/$REGRESSION_NAME.html"
MILESTONE_REPORT="$REPORT_DIR/$MILESTONE_NAME-report.html"
COMPARE_ERROR_REPORT="$REPORT_DIR/$COMPARE_ERROR_NAME.html"

cleanup() {
    # Both roots were created by this test and contain only disposable fixtures.
    python3 - "$PROJECT_ROOT" "$TEMP_BIN" <<'PY'
import shutil, sys
for path in sys.argv[1:]:
    shutil.rmtree(path)
PY
}
trap cleanup EXIT

# Keep this test independent from a GPU adapter and from launcher compilation.
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$TEMP_BIN/cargo"
printf '%s\n' '#!/usr/bin/env bash' 'exit 1' > "$TEMP_BIN/failing-launcher"
printf '%s\n' '#!/usr/bin/env python3' 'import os, shutil, sys' \
    'os.write(int(os.environ["AETHER_READY_FD"]), b"R")' \
    'shutil.copyfile("tests/reference/07_ssr_debug.png", sys.argv[4])' > "$TEMP_BIN/passing-launcher"
printf '%s\n' '#!/usr/bin/env bash' 'exit 1' > "$TEMP_BIN/failing-compare"
chmod +x "$TEMP_BIN/cargo" "$TEMP_BIN/failing-launcher"
chmod +x "$TEMP_BIN/passing-launcher" "$TEMP_BIN/failing-compare"

python3 - "$PROJECT_ROOT" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
(root / 'tests/visual-matrix.json').write_text(json.dumps({
    'scenes': [{'name': '07_ssr_debug', 'scene': 'fake-scene.ron'}]}))
(root / 'tests/reference/07_ssr_debug.png').write_bytes(b'fake PNG; comparator is stubbed')
PY

PATH="$TEMP_BIN:$PATH" AETHER_LAUNCHER_BIN="$TEMP_BIN/failing-launcher" "$PROJECT_ROOT/scripts/verify-regression.sh" \
    --scene 07_ssr_debug \
    --report "$REGRESSION_NAME" >/dev/null 2>&1 || true

PATH="$TEMP_BIN:$PATH" "$PROJECT_ROOT/scripts/verify-milestone.sh" \
    "$MILESTONE_NAME" >/dev/null 2>&1 || true

for report in "$REGRESSION_REPORT" "$MILESTONE_REPORT"; do
    test -f "$report"
    test ! -e "${report%.html}.md"
    rg -q '<!doctype html>' "$report"
    rg -q '<table>' "$report"
done

rg -q 'CRASH' "$REGRESSION_REPORT"
rg -q 'NO IMG' "$MILESTONE_REPORT"

AETHER_LAUNCHER_BIN="$TEMP_BIN/passing-launcher" AETHER_COMPARE_SCRIPT="$TEMP_BIN/failing-compare" \
    "$PROJECT_ROOT/scripts/verify-regression.sh" --scene 07_ssr_debug \
    --report "$COMPARE_ERROR_NAME" >/dev/null 2>&1 || true
test -f "$COMPARE_ERROR_REPORT"
rg -q 'COMPARE_ERROR' "$COMPARE_ERROR_REPORT"
