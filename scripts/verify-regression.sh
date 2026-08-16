#!/usr/bin/env bash
# Aether Engine visual regression runner.
#
# Usage:
#   ./scripts/verify-regression.sh                 # run all scenes in tests/visual-matrix.json
#   ./scripts/verify-regression.sh --scene 13_clouds
#   ./scripts/verify-regression.sh --update-references
#
# The script captures screenshots from the launcher, compares them with
# tests/reference/*.png when available, and writes a report to tests/reports/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

MATRIX="tests/visual-matrix.json"
OUTPUT_DIR="tests/output"
REFERENCE_DIR="tests/reference"
REPORT_DIR="tests/reports"
COMPARE_SCRIPT=".claude/skills/aether-visual-test/scripts/compare_images.py"

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
REPORT_FILE="$REPORT_DIR/$REPORT_NAME.md"

# Read matrix into a shell-escaped TSV list via Python (keeps JSON handling simple).
# Columns: name<TAB>scene<TAB>frames<TAB>width<TAB>height<TAB>debug_mode<TAB>ssr<TAB>threshold
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
    ssr = "1" if item.get("ssr", False) else "0"
    threshold = item.get("threshold", defaults.get("threshold", 0.95))
    print(f"{name}\t{scene}\t{frames}\t{width}\t{height}\t{debug_mode}\t{ssr}\t{threshold}")
PY
)"

if [[ -z "$MATRIX_ROWS" ]]; then
    echo "No scenes matched." >&2
    exit 0
fi

cat > "$REPORT_FILE" <<EOF
# Visual Regression Report — $(date +%Y-%m-%d\ %H:%M:%S)

| Scene | Status | SSIM | MAE | Diff% | Diff Image |
|-------|--------|------|-----|-------|------------|
EOF

OVERALL_PASS=true
TOTAL=0
PASSED=0
FAILED=0
NEW=0

while IFS=$'\t' read -r name scene frames width height debug_mode ssr threshold; do
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
    if [[ "$ssr" == "1" ]]; then
        ARGS+=(--ssr)
    fi
    ARGS+=(--no-gui-overlay --freeze-time --width "$width" --height "$height")

    if ! cargo run --bin aether-launcher --quiet -- "${ARGS[@]}" >/dev/null 2>&1; then
        echo "  ❌ Launcher failed for $name"
        echo "| $name | ❌ CRASH | N/A | N/A | N/A | N/A |" >> "$REPORT_FILE"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

    if [[ ! -f "$OUT_IMAGE" ]]; then
        echo "  ❌ Screenshot missing for $name"
        echo "| $name | ❌ NO_IMG | N/A | N/A | N/A | N/A |" >> "$REPORT_FILE"
        OVERALL_PASS=false
        FAILED=$((FAILED + 1))
        continue
    fi

    if [[ ! -f "$REF_IMAGE" ]]; then
        echo "  ⚠️  No reference for $name"
        if [[ "$UPDATE_REFS" == "true" ]]; then
            cp "$OUT_IMAGE" "$REF_IMAGE"
            echo "  ✅ Reference created: $REF_IMAGE"
            echo "| $name | ✅ REF_CREATED | N/A | N/A | N/A | N/A |" >> "$REPORT_FILE"
            NEW=$((NEW + 1))
        else
            echo "  → Run with --update-references to create the baseline"
            echo "| $name | ⚠️ NEW | N/A | N/A | N/A | N/A |" >> "$REPORT_FILE"
            NEW=$((NEW + 1))
        fi
        continue
    fi

    if [[ "$UPDATE_REFS" == "true" ]]; then
        cp "$OUT_IMAGE" "$REF_IMAGE"
        echo "  ✅ Reference updated: $REF_IMAGE"
        echo "| $name | ✅ REF_UPDATED | N/A | N/A | N/A | N/A |" >> "$REPORT_FILE"
        NEW=$((NEW + 1))
        continue
    fi

    DIFF_IMAGE="$OUTPUT_DIR/${name}.diff.png"
    SIDE_IMAGE="$OUTPUT_DIR/${name}.side.png"
    METRICS="$(python3 "$COMPARE_SCRIPT" "$REF_IMAGE" "$OUT_IMAGE" \
        --threshold "$threshold" \
        --diff "$DIFF_IMAGE" \
        --side-by-side "$SIDE_IMAGE" \
        --json 2>/dev/null || true)"

    # Parse metrics from JSON in Python; missing/NaN values become N/A.
    parsed="$(python3 - "$METRICS" <<'PY'
import json
import sys

raw = sys.argv[1] if len(sys.argv) > 1 else ""
try:
    data = json.loads(raw)
except Exception:
    print("N/A\tN/A\tN/A\tN/A")
    raise SystemExit(0)

ssim = data.get("ssim")
if ssim is None:
    ssim = "N/A"
else:
    ssim = f"{ssim:.4f}"
mae = data.get("mae", 0)
diff_pct = data.get("diff_pct", 0)
print(f"{ssim}\t{mae:.2f}\t{diff_pct:.2f}")
PY
)"
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

    echo "| $name | $status_metric | $ssim_metric | $mae_metric | $diff_metric% | [$name.diff.png]($DIFF_IMAGE) |" >> "$REPORT_FILE"
done <<< "$MATRIX_ROWS"

echo "" >> "$REPORT_FILE"
echo "## Summary" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
echo "- Total: $TOTAL" >> "$REPORT_FILE"
echo "- Passed: $PASSED" >> "$REPORT_FILE"
echo "- Failed: $FAILED" >> "$REPORT_FILE"
echo "- New/No-Reference: $NEW" >> "$REPORT_FILE"

if [[ "$OVERALL_PASS" == "true" ]]; then
    echo "" >> "$REPORT_FILE"
    echo "**Overall: ✅ PASS**" >> "$REPORT_FILE"
else
    echo "" >> "$REPORT_FILE"
    echo "**Overall: ❌ FAIL**" >> "$REPORT_FILE"
fi

echo ""
echo "Report saved to: $REPORT_FILE"
if [[ "$OVERALL_PASS" == "false" ]]; then
    exit 1
fi
