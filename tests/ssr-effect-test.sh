#!/usr/bin/env bash
# Verify that the clear SSR fixture changes when SSR is enabled.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

SCENE="${AETHER_SSR_SCENE:-scenes/21_ssr_debug_clear.ron}"
OUTPUT_STEM="${AETHER_SSR_OUTPUT_STEM:-21_ssr_debug_clear}"
OUTPUT_DIR="tests/output"
REPORT_DIR="tests/reports"
COMPARE_SCRIPT="${AETHER_COMPARE_SCRIPT:-.claude/skills/aether-visual-test/scripts/compare_images.py}"
REPORT_NAME="${1:-$(date +%Y%m%d-%H%M%S)-ssr-effect}"
REPORT_FILE="$REPORT_DIR/$REPORT_NAME.html"
OFF_IMAGE="$OUTPUT_DIR/${OUTPUT_STEM}_off.png"
ON_IMAGE="$OUTPUT_DIR/${OUTPUT_STEM}_on.png"
DIFF_IMAGE="$OUTPUT_DIR/${OUTPUT_STEM}.effect.diff.png"
SIDE_IMAGE="$OUTPUT_DIR/${OUTPUT_STEM}.effect.side.png"

if [[ -n "${AETHER_LAUNCHER_BIN:-}" ]]; then
    LAUNCHER_COMMAND=("$AETHER_LAUNCHER_BIN")
else
    LAUNCHER_COMMAND=(cargo run --bin aether-launcher --quiet --)
fi

html_escape() {
    printf '%s' "$1" | sed \
        -e 's/&/\&amp;/g' \
        -e 's/</\&lt;/g' \
        -e 's/>/\&gt;/g' \
        -e 's/"/\&quot;/g'
}

mkdir -p "$OUTPUT_DIR" "$REPORT_DIR"
rm -f "$OFF_IMAGE" "$ON_IMAGE" "$DIFF_IMAGE" "$SIDE_IMAGE"

capture() {
    local output="$1"
    shift
    "${LAUNCHER_COMMAND[@]}" \
        --scene "$SCENE" \
        --screenshot "$output" \
        --exit-after-frames 1 \
        --no-gui-overlay \
        --freeze-time \
        --width 1280 \
        --height 720 \
        "$@" >/dev/null 2>&1
}

overall="FAIL"
status="❌ FAIL"
metrics=""
error_message=""

if ! capture "$OFF_IMAGE"; then
    error_message="SSR-off launcher failed"
elif [[ ! -f "$OFF_IMAGE" ]]; then
    error_message="SSR-off screenshot missing"
elif ! capture "$ON_IMAGE" --ssr; then
    error_message="SSR-on launcher failed"
elif [[ ! -f "$ON_IMAGE" ]]; then
    error_message="SSR-on screenshot missing"
elif ! metrics="$(python3 "$COMPARE_SCRIPT" "$OFF_IMAGE" "$ON_IMAGE" \
    --threshold 0.0 \
    --diff "$DIFF_IMAGE" \
    --side-by-side "$SIDE_IMAGE" \
    --json 2>/dev/null)"; then
    error_message="image comparator failed"
else
    read -r mae diff_pct width height <<<"$(python3 - "$metrics" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
print(data.get("mae", "N/A"), data.get("diff_pct", "N/A"), data.get("width", "N/A"), data.get("height", "N/A"))
PY
)"
    min_diff="${AETHER_SSR_MIN_DIFF_PCT:-0.5}"
    if python3 - "$diff_pct" "$min_diff" <<'PY'
import sys
sys.exit(0 if float(sys.argv[1]) >= float(sys.argv[2]) else 1)
PY
    then
        overall="PASS"
        status="✅ EFFECT DETECTED"
    else
        error_message="SSR on/off difference ${diff_pct}% is below minimum ${min_diff}%"
    fi
fi

cat > "$REPORT_FILE" <<EOF
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>SSR Effect Test — $REPORT_NAME</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { margin: 0; padding: 2rem; background: Canvas; color: CanvasText; }
    main { max-width: 72rem; margin: 0 auto; }
    .meta { color: GrayText; }
    table { width: 100%; border-collapse: collapse; margin-top: 1.5rem; }
    th, td { padding: .65rem .8rem; border-bottom: 1px solid ButtonBorder; text-align: left; }
    th { background: color-mix(in srgb, CanvasText 10%, Canvas); }
    .pass { color: #188038; font-weight: 600; }
    .fail { color: #c5221f; font-weight: 600; }
    .gallery { display: grid; grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr)); gap: 1rem; margin-top: 1.5rem; }
    figure { margin: 0; overflow: hidden; border: 1px solid ButtonBorder; border-radius: .5rem; }
    img { display: block; width: 100%; height: auto; }
    figcaption { padding: .7rem .8rem; }
  </style>
</head>
<body>
  <main>
    <h1>SSR Effect Test</h1>
    <p class="meta">Scene: <code>$SCENE</code> · frozen 1280×720 single frame</p>
    <table>
      <thead><tr><th>Comparison</th><th>Status</th><th>MAE</th><th>Diff%</th><th>Resolution</th><th>Details</th></tr></thead>
      <tbody>
        <tr><td>SSR OFF → SSR ON</td><td class="$(if [[ "$overall" == "PASS" ]]; then printf 'pass'; else printf 'fail'; fi)">$status</td><td>$(html_escape "${mae:-N/A}")</td><td>$(html_escape "${diff_pct:-N/A}")</td><td>$(html_escape "${width:-N/A}×${height:-N/A}")</td><td>$(html_escape "${error_message:-SSR output changed above the configured minimum}")</td></tr>
      </tbody>
    </table>
    <div class="gallery">
      $(if [[ -f "$OFF_IMAGE" ]]; then printf '<figure><a href="../output/%s"><img src="../output/%s" alt="SSR disabled output"></a><figcaption>SSR OFF</figcaption></figure>' "$(basename "$OFF_IMAGE")" "$(basename "$OFF_IMAGE")"; fi)
      $(if [[ -f "$ON_IMAGE" ]]; then printf '<figure><a href="../output/%s"><img src="../output/%s" alt="SSR enabled output"></a><figcaption>SSR ON</figcaption></figure>' "$(basename "$ON_IMAGE")" "$(basename "$ON_IMAGE")"; fi)
      $(if [[ -f "$SIDE_IMAGE" ]]; then printf '<figure><a href="../output/%s"><img src="../output/%s" alt="SSR off and on comparison"></a><figcaption>Side-by-side</figcaption></figure>' "$(basename "$SIDE_IMAGE")" "$(basename "$SIDE_IMAGE")"; fi)
      $(if [[ -f "$DIFF_IMAGE" ]]; then printf '<figure><a href="../output/%s"><img src="../output/%s" alt="SSR effect diff"></a><figcaption>Diff mask</figcaption></figure>' "$(basename "$DIFF_IMAGE")" "$(basename "$DIFF_IMAGE")"; fi)
    </div>
  </main>
</body>
</html>
EOF

if [[ "$overall" == "PASS" ]]; then
    echo "✅ SSR effect detected: MAE=$mae Diff=${diff_pct}% (${width}x${height})"
    echo "Report: $REPORT_FILE"
    exit 0
fi

echo "❌ SSR effect test failed: ${error_message:-unknown error}" >&2
echo "Report: $REPORT_FILE" >&2
exit 1
