#!/usr/bin/env bash
# 一键验证当月/里程碑的所有场景
# 用法: ./scripts/verify-milestone.sh [report_name]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

REPORT_NAME="${1:-$(date +%Y%m%d-%H%M%S)}"
REPORT_DIR="tests/reports"
OUTPUT_DIR="tests/output"
REFERENCE_DIR="tests/reference"
REPORT_FILE="$REPORT_DIR/$REPORT_NAME-report.html"

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

status_class() {
    case "$1" in
        *"❌"*) printf 'fail' ;;
        *"✅"*) printf 'pass' ;;
        *) printf 'warn' ;;
    esac
}

append_result_row() {
    local name="$1"
    local debug="$2"
    local frames="$3"
    local ssim="$4"
    local mae="$5"
    local diff="$6"
    local status="$7"

    printf '        <tr class="%s"><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td></tr>\n' \
        "$(status_class "$status")" \
        "$(html_escape "$name")" \
        "$(html_escape "$debug")" \
        "$(html_escape "$frames")" \
        "$(html_escape "$ssim")" \
        "$(html_escape "$mae")" \
        "$(html_escape "$diff")" \
        "$(html_escape "$status")" >> "$REPORT_FILE"
}

mkdir -p "$OUTPUT_DIR" "$REPORT_DIR"

# 场景列表: (场景文件 输出名 帧数 debug_mode ssao_enabled)
SCENES=(
    "scenes/01_deferred.ron       01_deferred       60   0  0"
    "scenes/02_multi_object.ron   02_multi_object   60   0  0"
    "scenes/03_shadow_demo.ron    03_shadow_demo    120  0  0"
    "scenes/03_shadow_demo.ron    03_shadow_debug   120  6  0"
    "scenes/04_ibl_debug.ron      04_ibl_debug      120  0  0"
    "scenes/05_ssao_debug.ron     05_ssao_debug_mode14     1  14 1"
    "scenes/06_ssao_extreme.ron   06_ssao_extreme_mode14   1  14 1"
    "scenes/07_ssr_debug.ron      07_ssr_debug      120  0  0"
)

echo "=== Aether Engine Visual Milestone Verification ==="
echo "Report: $REPORT_FILE"
echo ""

REPORT_TIMESTAMP="$(date +%Y-%m-%d\ %H:%M:%S)"
cat > "$REPORT_FILE" <<EOF
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Visual Milestone Report — $REPORT_TIMESTAMP</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { margin: 0; padding: 2rem; background: Canvas; color: CanvasText; }
    main { max-width: 72rem; margin: 0 auto; }
    .meta { color: GrayText; }
    table { width: 100%; border-collapse: collapse; margin-top: 1.5rem; }
    th, td { padding: 0.65rem 0.8rem; border-bottom: 1px solid ButtonBorder; text-align: left; }
    th { background: color-mix(in srgb, CanvasText 10%, Canvas); }
    tr.pass td:last-child { color: #188038; }
    tr.warn td:last-child { color: #b06000; }
    tr.fail td:last-child { color: #c5221f; }
    .summary { margin-top: 1.5rem; padding: 1rem; border: 1px solid ButtonBorder; border-radius: 0.5rem; }
  </style>
</head>
<body>
  <main>
    <h1>Visual Milestone Report</h1>
    <p class="meta">Generated: $REPORT_TIMESTAMP</p>
    <table>
      <thead>
        <tr><th scope="col">Scene</th><th scope="col">Debug</th><th scope="col">Frames</th><th scope="col">SSIM</th><th scope="col">MAE</th><th scope="col">Diff%</th><th scope="col">Status</th></tr>
      </thead>
      <tbody>
EOF

OVERALL_PASS=true

for entry in "${SCENES[@]}"; do
    read -r scene_file out_name frames debug_mode ssao_enabled <<< "$entry"

    # 07_ssr_debug needs SSR enabled so reflection improvements are verified.
    ssr_flag=""
    if [[ "$out_name" == "07_ssr_debug" ]]; then
        ssr_flag="--ssr"
    fi

    ssao_flag=""
    if [[ "$ssao_enabled" == "1" ]]; then
        ssao_flag="--ssao"
    fi

    echo "▶ Testing: $out_name (scene=$scene_file, frames=$frames, debug=$debug_mode)"

    ARGS=(
        --scene "$scene_file" \
        --screenshot "$OUTPUT_DIR/${out_name}.png" \
        --exit-after-frames "$frames" \
        --no-gui-overlay \
        --freeze-time \
        --debug-mode "$debug_mode" \
        --width 2560 \
        --height 1440
    )
    if [[ -n "$ssao_flag" ]]; then
        ARGS+=("$ssao_flag")
    fi
    if [[ -n "$ssr_flag" ]]; then
        ARGS+=("$ssr_flag")
    fi

    rm -f "$OUTPUT_DIR/${out_name}.png"
    if ! "${LAUNCHER_COMMAND[@]}" "${ARGS[@]}" 2>/dev/null; then
        echo "  ❌ Launcher failed for $out_name"
        append_result_row "$out_name" "$debug_mode" "$frames" "N/A" "N/A" "N/A" "❌ CRASH"
        OVERALL_PASS=false
        continue
    fi

    if [[ ! -f "$OUTPUT_DIR/${out_name}.png" ]]; then
        echo "  ❌ Screenshot missing for $out_name"
        append_result_row "$out_name" "$debug_mode" "$frames" "N/A" "N/A" "N/A" "❌ NO IMG"
        OVERALL_PASS=false
        continue
    fi

    # 对比参考图（如存在）
    if [[ -f "$REFERENCE_DIR/${out_name}.png" ]]; then
        if ! result=$(python3 .claude/skills/aether-visual-test/scripts/compare_images.py \
            "$REFERENCE_DIR/${out_name}.png" \
            "$OUTPUT_DIR/${out_name}.png" \
            --json 2>/dev/null); then
            echo "  ❌ COMPARE_ERROR for $out_name"
            append_result_row "$out_name" "$debug_mode" "$frames" "N/A" "N/A" "N/A" "❌ COMPARE_ERROR"
            OVERALL_PASS=false
            continue
        fi

        ssim=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('ssim','N/A'))")
        mae=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('mae',0))")
        diff_pct=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('diff_pct',0))")

        if [[ "$ssim" == "None" || "$ssim" == "null" ]]; then
            ssim_str="N/A"
            if python3 -c "import sys; sys.exit(0 if float('$diff_pct') <= 2.0 else 1)"; then
                status="✅ PASS"
            else
                status="❌ REGRESSION"
                OVERALL_PASS=false
            fi
        else
            ssim_str=$(printf "%.4f" "$ssim")
            if python3 -c "import sys; sys.exit(0 if float('$ssim') >= 0.95 else 1)"; then
                status="✅ PASS"
            else
                status="❌ REGRESSION"
                OVERALL_PASS=false
            fi
        fi

        echo "  SSIM=$ssim_str MAE=$mae Diff=${diff_pct}% → $status"
        append_result_row "$out_name" "$debug_mode" "$frames" "$ssim_str" "$mae" "${diff_pct}%" "$status"
    else
        echo "  ⚠️  No reference image — manual inspection required"
        append_result_row "$out_name" "$debug_mode" "$frames" "N/A" "N/A" "N/A" "⚠️ NO REF"
    fi
done

cat >> "$REPORT_FILE" <<EOF
      </tbody>
    </table>
    <section class="summary">
      <h2>Summary</h2>
      <p>Overall: $(if $OVERALL_PASS; then printf '✅ PASS'; else printf '❌ FAIL'; fi)</p>
    </section>
  </main>
</body>
</html>
EOF

if $OVERALL_PASS; then
    echo ""
    echo "=== ✅ All scenes passed ==="
else
    echo ""
    echo "=== ❌ Some scenes failed — see report ==="
fi

echo "Report saved to: $REPORT_FILE"
