#!/usr/bin/env bash
# Aether Engine visual regression runner.
#
# Usage:
#   ./scripts/verify-regression.sh                 # run all scenes in tests/visual-matrix.json
#   ./scripts/verify-regression.sh --scene 13_clouds
#   ./scripts/verify-regression.sh --update-references --reason "approved baseline refresh"
#
# The script captures screenshots from the launcher, compares them with
# tests/reference/*.png when available, and writes an HTML report to
# tests/reports/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

MATRIX="tests/visual-matrix.json"
OUTPUT_DIR="tests/output"
REFERENCE_DIR="tests/reference"
REPORT_DIR="tests/reports"
COMPARE_SCRIPT="${AETHER_COMPARE_SCRIPT:-.claude/skills/aether-visual-test/scripts/compare_images.py}"
# Each matrix entry launches a fresh wgpu device. macOS Metal may finish
# releasing the previous window/device asynchronously after the launcher exits;
# a short settle period prevents that teardown from contaminating the next
# process' first frames. Set to 0 only when intentionally bypassing the guard.
REGRESSION_SETTLE_SECONDS="${AETHER_REGRESSION_SETTLE_SECONDS:-3}"

FILTER=""
UPDATE_REFS=false
REFERENCE_REASON=""
REPORT_NAME=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --scene)
            FILTER="$2"
            shift 2
            ;;
        --update-references)
            UPDATE_REFS=true
            shift
            ;;
        --reason|--reference-reason)
            [[ $# -ge 2 ]] || { echo "Missing value for $1" >&2; exit 2; }
            REFERENCE_REASON="$2"
            shift 2
            ;;
        --report)
            REPORT_NAME="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            echo "Usage: $0 [--scene NAME] [--update-references --reason TEXT] [--report NAME]" >&2
            exit 2
            ;;
    esac
done

if [[ "$UPDATE_REFS" == "true" && -z "$REFERENCE_REASON" ]]; then
    echo "--update-references requires --reason TEXT" >&2
    exit 2
fi
if [[ "$UPDATE_REFS" != "true" && -n "$REFERENCE_REASON" ]]; then
    echo "--reason is only valid with --update-references" >&2
    exit 2
fi

mkdir -p "$OUTPUT_DIR" "$REFERENCE_DIR" "$REPORT_DIR"

REPORT_NAME="${REPORT_NAME:-$(date +%Y%m%d-%H%M%S)-visual-regression}"
REPORT_FILE="$REPORT_DIR/$REPORT_NAME.html"

if [[ -n "${AETHER_LAUNCHER_BIN:-}" ]]; then
    LAUNCHER_BIN="$AETHER_LAUNCHER_BIN"
else
    cargo build --bin aether-launcher --quiet
    TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    LAUNCHER_BIN="$TARGET_DIR/debug/aether-launcher"
fi

ACTIVE_RUNNER_PID=""

cleanup_active_runner() {
    if [[ -n "$ACTIVE_RUNNER_PID" ]]; then
        kill -TERM "$ACTIVE_RUNNER_PID" 2>/dev/null || true
        wait "$ACTIVE_RUNNER_PID" 2>/dev/null || true
    fi
    ACTIVE_RUNNER_PID=""
}

abort_regression() {
    trap '' INT TERM
    cleanup_active_runner
    exit "$1"
}

trap cleanup_active_runner EXIT
trap 'abort_regression 130' INT
trap 'abort_regression 143' TERM

html_escape() {
    printf '%s' "$1" | sed \
        -e 's/&/\&amp;/g' \
        -e 's/</\&lt;/g' \
        -e 's/>/\&gt;/g' \
        -e 's/"/\&quot;/g'
}

status_class() {
    case "$1" in
        *"❌"*) printf 'fail' ;;
        *"✅"*) printf 'pass' ;;
        *) printf 'warn' ;;
    esac
}

append_result_row() {
    local name="$1"
    local status="$2"
    local ssim="$3"
    local mae="$4"
    local diff="$5"
    local diff_url="${6:-}"
    local side_url="${7:-}"
    local image_cell="—"

    if [[ -n "$diff_url" ]]; then
        local escaped_diff_url
        escaped_diff_url="$(html_escape "$diff_url")"
        image_cell="<a href=\"$escaped_diff_url\">diff</a>"
        if [[ -n "$side_url" ]]; then
            local escaped_side_url
            escaped_side_url="$(html_escape "$side_url")"
            image_cell+=" · <a href=\"$escaped_side_url\">side</a>"
        fi
    fi

    printf '        <tr class="%s"><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td></tr>\n' \
        "$(status_class "$status")" \
        "$(html_escape "$name")" \
        "$(html_escape "$status")" \
        "$(html_escape "$ssim")" \
        "$(html_escape "$mae")" \
        "$(html_escape "$diff")" \
        "$image_cell" >> "$REPORT_FILE"
}

# Read matrix into a shell-escaped TSV list via Python (keeps JSON handling simple).
# Columns: name<TAB>scene<TAB>frames<TAB>width<TAB>height<TAB>debug_mode<TAB>ssao<TAB>ssr<TAB>threshold
MATRIX_ROWS="$(python3 - "$MATRIX" "$FILTER" <<'PY'
import json
import sys

matrix_path = sys.argv[1]
filter_name = sys.argv[2] if len(sys.argv) > 2 else ""

with open(matrix_path, "r", encoding="utf-8") as f:
    data = json.load(f)

defaults = data.get("defaults", {})
for item in data.get("scenes", []):
    name = item.get("name", "")
    if filter_name and name != filter_name:
        continue
    scene = item.get("scene", "")
    frames = item.get("frames", defaults.get("frames", 60))
    width = item.get("width", defaults.get("width", 1280))
    height = item.get("height", defaults.get("height", 720))
    debug_mode = item.get("debug_mode") or "none"
    ssao = "1" if item.get("ssao", False) else "0"
    ssr = "1" if item.get("ssr", False) else "0"
    threshold = item.get("threshold", defaults.get("threshold", 0.95))
    print(f"{name}\t{scene}\t{frames}\t{width}\t{height}\t{debug_mode}\t{ssao}\t{ssr}\t{threshold}")
PY
)"

if [[ -z "$MATRIX_ROWS" ]]; then
    echo "No scenes matched." >&2
    exit 0
fi

REPORT_TIMESTAMP="$(date +%Y-%m-%d\ %H:%M:%S)"
REFERENCE_ACTION="compare"
if [[ "$UPDATE_REFS" == "true" ]]; then
    REFERENCE_ACTION="update"
fi
ESCAPED_REFERENCE_REASON="$(html_escape "${REFERENCE_REASON:-not requested}")"
cat > "$REPORT_FILE" <<EOF
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Visual Regression Report — $REPORT_TIMESTAMP</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { margin: 0; padding: 2rem; background: Canvas; color: CanvasText; }
    main { max-width: 72rem; margin: 0 auto; }
    .meta { color: GrayText; }
    table { width: 100%; border-collapse: collapse; margin-top: 1.5rem; }
    th, td { padding: 0.65rem 0.8rem; border-bottom: 1px solid ButtonBorder; text-align: left; }
    th { background: color-mix(in srgb, CanvasText 10%, Canvas); }
    tr.pass td:nth-child(2) { color: #188038; }
    tr.warn td:nth-child(2) { color: #b06000; }
    tr.fail td:nth-child(2) { color: #c5221f; }
    .summary { margin-top: 1.5rem; padding: 1rem; border: 1px solid ButtonBorder; border-radius: 0.5rem; }
  </style>
</head>
<body>
  <main>
    <h1>Visual Regression Report</h1>
    <p class="meta">Generated: $REPORT_TIMESTAMP</p>
    <p class="meta">Reference action: $REFERENCE_ACTION · Reason: $ESCAPED_REFERENCE_REASON</p>
    <table>
      <thead>
        <tr><th scope="col">Scene</th><th scope="col">Status</th><th scope="col">SSIM</th><th scope="col">MAE</th><th scope="col">Diff%</th><th scope="col">Images</th></tr>
      </thead>
      <tbody>
EOF

OVERALL_PASS=true
TOTAL=0
PASSED=0
FAILED=0
NEW=0

# One coordinator owns every launcher lease for this invocation. Comparisons
# below retain the v1 HTML/status/threshold behavior and output paths.
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$-$RANDOM"
RUN_REPORT_DIR="$REPORT_DIR/$RUN_ID"
RUNNER_ARGS=(--launcher "$LAUNCHER_BIN" --matrix "$MATRIX"
    --report-dir "$RUN_REPORT_DIR" --output-dir "$OUTPUT_DIR"
    --settle-seconds "$REGRESSION_SETTLE_SECONDS")
if [[ -n "$FILTER" ]]; then
    RUNNER_ARGS+=(--case "$FILTER")
fi
python3 "$SCRIPT_DIR/runner_process.py" "${RUNNER_ARGS[@]}" &
ACTIVE_RUNNER_PID=$!
runner_status=0
wait "$ACTIVE_RUNNER_PID" || runner_status=$?
ACTIVE_RUNNER_PID=""
if [[ "$runner_status" -ge 128 ]]; then
    exit "$runner_status"
fi

while IFS=$'\t' read -r name scene frames width height debug_mode ssao ssr threshold; do
    TOTAL=$((TOTAL + 1))
    echo ""
    echo "▶ [$name] $scene"

    OUT_IMAGE="$OUTPUT_DIR/$name.png"
    REF_IMAGE="$REFERENCE_DIR/$name.png"

    if ! python3 - "$RUN_REPORT_DIR/$name/launcher.log" <<'PY'
import json
import sys
try:
    with open(sys.argv[1]) as stream:
        record = json.load(stream)
    ok = record['diagnostic'] is None and record['group_empty'] and record['exit_code'] == 0
except (OSError, ValueError, KeyError):
    ok = False
raise SystemExit(0 if ok else 1)
PY
    then
        echo "  ❌ Launcher failed for $name"
        append_result_row "$name" "❌ CRASH" "N/A" "N/A" "N/A"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

    if [[ ! -f "$OUT_IMAGE" ]]; then
        echo "  ❌ Screenshot missing for $name"
        append_result_row "$name" "❌ NO_IMG" "N/A" "N/A" "N/A"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

    if [[ ! -f "$REF_IMAGE" ]]; then
        echo "  ⚠️  No reference for $name"
        if [[ "$UPDATE_REFS" == "true" ]]; then
            cp "$OUT_IMAGE" "$REF_IMAGE"
            echo "  ✅ Reference created: $REF_IMAGE"
            append_result_row "$name" "✅ REF_CREATED" "N/A" "N/A" "N/A"
            NEW=$((NEW + 1))
        else
            echo "  → Run with --update-references to create the baseline"
            append_result_row "$name" "⚠️ NEW" "N/A" "N/A" "N/A"
            NEW=$((NEW + 1))
        fi
        continue
    fi

    if [[ "$UPDATE_REFS" == "true" ]]; then
        cp "$OUT_IMAGE" "$REF_IMAGE"
        echo "  ✅ Reference updated: $REF_IMAGE"
        append_result_row "$name" "✅ REF_UPDATED" "N/A" "N/A" "N/A"
        NEW=$((NEW + 1))
        continue
    fi

    DIFF_IMAGE="$OUTPUT_DIR/${name}.diff.png"
    SIDE_IMAGE="$OUTPUT_DIR/${name}.side.png"
    if ! METRICS="$(python3 "$COMPARE_SCRIPT" "$REF_IMAGE" "$OUT_IMAGE" \
        --threshold "$threshold" \
        --diff "$DIFF_IMAGE" \
        --side-by-side "$SIDE_IMAGE" \
        --json 2>/dev/null)"; then
        echo "  ❌ COMPARE_ERROR for $name"
        append_result_row "$name" "❌ COMPARE_ERROR" "N/A" "N/A" "N/A"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

# Parse metrics from JSON in Python; missing/non-finite values become N/A.
# At least one decision metric (SSIM or Diff%) must be present.
    if ! parsed="$(python3 - "$METRICS" <<'PY'
import json
import math
import sys

raw = sys.argv[1] if len(sys.argv) > 1 else ""
try:
    data = json.loads(raw)
except Exception:
    raise SystemExit(1)

def finite_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)

ssim_value = data.get("ssim")
mae_value = data.get("mae")
diff_value = data.get("diff_pct")
if not finite_number(ssim_value) and not finite_number(diff_value):
    raise SystemExit(1)
ssim = f"{ssim_value:.4f}" if finite_number(ssim_value) else "N/A"
mae = f"{mae_value:.2f}" if finite_number(mae_value) else "N/A"
diff_pct = f"{diff_value:.2f}" if finite_number(diff_value) else "N/A"
print(f"{ssim}\t{mae}\t{diff_pct}")
PY
    )"; then
        echo "  ❌ COMPARE_ERROR for $name (invalid metrics)"
        append_result_row "$name" "❌ COMPARE_ERROR" "N/A" "N/A" "N/A"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi
    ssim_metric="$(echo "$parsed" | cut -f1)"
    mae_metric="$(echo "$parsed" | cut -f2)"
    diff_metric="$(echo "$parsed" | cut -f3)"

    # Determine status. SSIM is preferred; fall back to diff% when scikit-image
    # is not installed. For deterministic freeze-frame captures, a near-zero
    # diff% is expected for a clean refactor.
    status_metric="✅ PASS"
    if [[ "$ssim_metric" != "N/A" ]]; then
        if python3 - "$ssim_metric" "$threshold" <<'PY'
import sys
try:
    ok = float(sys.argv[1]) >= float(sys.argv[2])
except Exception:
    ok = True
raise SystemExit(0 if ok else 1)
PY
        then
            :
        else
            status_metric="❌ REGRESSION"
        fi
    elif [[ "$diff_metric" != "N/A" ]] && python3 - "$diff_metric" <<'PY'
import sys
# Without scikit-image we use a looser fallback because identical-looking
# screenshots can still have ~1% pixels with small numeric differences.
try:
    ok = float(sys.argv[1]) <= 2.0
except Exception:
    ok = True
raise SystemExit(0 if ok else 1)
PY
    then
        :
    else
        status_metric="❌ REGRESSION"
    fi

    echo "  $status_metric SSIM=$ssim_metric MAE=$mae_metric Diff=$diff_metric%"
    echo "  diff: $DIFF_IMAGE"
    echo "  side: $SIDE_IMAGE"

    if [[ "$status_metric" == *"❌"* ]]; then
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
    else
        PASSED=$((PASSED + 1))
    fi

    append_result_row "$name" "$status_metric" "$ssim_metric" "$mae_metric" "$diff_metric%" \
        "../output/$(basename "$DIFF_IMAGE")" "../output/$(basename "$SIDE_IMAGE")"
done <<< "$MATRIX_ROWS"

cat >> "$REPORT_FILE" <<EOF
      </tbody>
    </table>
    <section class="summary">
      <h2>Summary</h2>
      <p>Total: $TOTAL · Passed: $PASSED · Failed: $FAILED · New/No-Reference: $NEW</p>
      <p>Overall: $(if [[ "$OVERALL_PASS" == "true" ]]; then printf '✅ PASS'; else printf '❌ FAIL'; fi)</p>
    </section>
  </main>
</body>
</html>
EOF

echo ""
echo "Report saved to: $REPORT_FILE"
if [[ "$OVERALL_PASS" == "false" ]]; then
    exit 1
fi
