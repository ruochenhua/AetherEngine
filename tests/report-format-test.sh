#!/usr/bin/env bash
# Smoke-test that report-producing verification scripts emit HTML.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
REPORT_DIR="$PROJECT_ROOT/tests/reports"
TEMP_BIN="$(mktemp -d)"
REGRESSION_NAME="report-format-regression-$$"
MILESTONE_NAME="report-format-milestone-$$"
COMPARE_ERROR_NAME="report-format-compare-error-$$"
REGRESSION_REPORT="$REPORT_DIR/$REGRESSION_NAME.html"
MILESTONE_REPORT="$REPORT_DIR/$MILESTONE_NAME-report.html"
COMPARE_ERROR_REPORT="$REPORT_DIR/$COMPARE_ERROR_NAME.html"

cleanup() {
    rm -f "$REGRESSION_REPORT" "$MILESTONE_REPORT"
    rm -f "$COMPARE_ERROR_REPORT"
    rmdir "$TEMP_BIN" 2>/dev/null || true
}
trap cleanup EXIT

# Keep this test independent from a GPU adapter and from launcher compilation.
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$TEMP_BIN/cargo"
printf '%s\n' '#!/usr/bin/env bash' 'exit 1' > "$TEMP_BIN/failing-launcher"
printf '%s\n' '#!/usr/bin/env bash' 'cp tests/reference/07_ssr_debug.png "$4"' > "$TEMP_BIN/passing-launcher"
printf '%s\n' '#!/usr/bin/env bash' 'exit 1' > "$TEMP_BIN/failing-compare"
chmod +x "$TEMP_BIN/cargo" "$TEMP_BIN/failing-launcher"
chmod +x "$TEMP_BIN/passing-launcher" "$TEMP_BIN/failing-compare"

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
