#!/usr/bin/env bash
# Aether Engine visual regression runner.
#
# Usage:
#   ./scripts/verify-regression.sh                 # run all scenes in tests/visual-matrix.json
#   ./scripts/verify-regression.sh --scene 13_clouds
#   ./scripts/verify-regression.sh --update-references
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
        --report)
            REPORT_NAME="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            echo "Usage: $0 [--scene NAME] [--update-references] [--report NAME]" >&2
            exit 2
            ;;
    esac
done

mkdir -p "$OUTPUT_DIR" "$REFERENCE_DIR" "$REPORT_DIR"

REPORT_NAME="${REPORT_NAME:-$(date +%Y%m%d-%H%M%S)-visual-regression}"
REPORT_FILE="$REPORT_DIR/$REPORT_NAME.html"

if [[ -n "${AETHER_LAUNCHER_BIN:-}" ]]; then
    LAUNCHER_COMMAND=("$AETHER_LAUNCHER_BIN")
else
    LAUNCHER_COMMAND=(cargo run --bin aether-launcher --quiet --)
fi

ACTIVE_LAUNCHER_PID=""

cleanup_active_launcher() {
    if [[ -n "$ACTIVE_LAUNCHER_PID" ]] && kill -0 "$ACTIVE_LAUNCHER_PID" 2>/dev/null; then
        kill -TERM "$ACTIVE_LAUNCHER_PID" 2>/dev/null || true
        wait "$ACTIVE_LAUNCHER_PID" 2>/dev/null || true
    fi
    ACTIVE_LAUNCHER_PID=""
}

abort_regression() {
    cleanup_active_launcher
    exit 130
}

trap cleanup_active_launcher EXIT
trap abort_regression INT TERM

run_launcher() {
    local status=0
    ("${LAUNCHER_COMMAND[@]}" "$@" >/dev/null 2>&1) &
    ACTIVE_LAUNCHER_PID=$!
    wait "$ACTIVE_LAUNCHER_PID" || status=$?
    ACTIVE_LAUNCHER_PID=""
    return "$status"
}

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

while IFS=$'\t' read -r name scene frames width height debug_mode ssao ssr threshold; do
    TOTAL=$((TOTAL + 1))
    echo ""
    echo "▶ [$name] $scene"

    OUT_IMAGE="$OUTPUT_DIR/$name.png"
    REF_IMAGE="$REFERENCE_DIR/$name.png"

    # Clean old output before capturing, so a missing screenshot is detected.
    rm -f "$OUT_IMAGE"

    ARGS=(--scene "$scene" --screenshot "$OUT_IMAGE" --exit-after-frames "$frames")
    if [[ "$debug_mode" != "none" ]]; then
        ARGS+=(--debug-mode "$debug_mode")
    fi
    if [[ "$ssao" == "1" ]]; then
        ARGS+=(--ssao)
    fi
    if [[ "$ssr" == "1" ]]; then
        ARGS+=(--ssr)
    fi
    ARGS+=(--no-gui-overlay --freeze-time --width "$width" --height "$height")

    launcher_status=0
    if run_launcher "${ARGS[@]}"; then
        launcher_status=0
    else
        launcher_status=$?
    fi
    if [[ "$launcher_status" -ne 0 ]]; then
        echo "  ❌ Launcher failed for $name"
        append_result_row "$name" "❌ CRASH" "N/A" "N/A" "N/A"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

    if [[ "$REGRESSION_SETTLE_SECONDS" != "0" ]]; then
        sleep "$REGRESSION_SETTLE_SECONDS"
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

    # Parse metrics from JSON in Python; missing/NaN values become N/A.
    if ! parsed="$(python3 - "$METRICS" <<'PY'
import json
import sys

raw = sys.argv[1] if len(sys.argv) > 1 else ""
try:
    data = json.loads(raw)
except Exception:
    raise SystemExit(1)

ssim = data.get("ssim")
if ssim is None:
    ssim = "N/A"
else:
    ssim = f"{ssim:.4f}"
mae = data.get("mae", 0)
diff_pct = data.get("diff_pct", 0)
print(f"{ssim}\t{mae:.2f}\t{diff_pct:.2f}")
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
    elif python3 - "$diff_metric" <<'PY'
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
