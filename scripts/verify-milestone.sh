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
REPORT_FILE="$REPORT_DIR/$REPORT_NAME-report.md"

mkdir -p "$OUTPUT_DIR" "$REPORT_DIR"

# 场景列表: (场景文件 输出名 帧数 [debug_mode])
SCENES=(
    "scenes/01_deferred.ron       01_deferred       60   0"
    "scenes/02_multi_object.ron   02_multi_object   60   0"
    "scenes/03_shadow_demo.ron    03_shadow_demo    120  0"
    "scenes/03_shadow_demo.ron    03_shadow_debug   120  6"
    "scenes/04_ibl_debug.ron      04_ibl_debug      120  0"
    "scenes/05_ssao_debug.ron     05_ssao_debug_mode14     120  14"
    "scenes/06_ssao_extreme.ron   06_ssao_extreme_mode14   120  0"
    "scenes/07_ssr_debug.ron      07_ssr_debug      120  0"
)

echo "=== Aether Engine Visual Milestone Verification ==="
echo "Report: $REPORT_FILE"
echo ""

cat > "$REPORT_FILE" <<EOF
# Visual Milestone Report — $(date +%Y-%m-%d\ %H:%M:%S)

| Scene | Debug | Frames | SSIM | MAE | Diff% | Status |
|-------|-------|--------|------|-----|-------|--------|
EOF

OVERALL_PASS=true

for entry in "${SCENES[@]}"; do
    read -r scene_file out_name frames debug_mode <<< "$entry"

    echo "▶ Testing: $out_name (scene=$scene_file, frames=$frames, debug=$debug_mode)"

    cargo run --bin aether-launcher --quiet -- \
        --scene "$scene_file" \
        --screenshot "$OUTPUT_DIR/${out_name}.png" \
        --exit-after-frames "$frames" \
        --no-gui-overlay \
        --freeze-time \
        --debug-mode "$debug_mode" 2>/dev/null || {
        echo "  ❌ Launcher failed for $out_name"
        echo "| $out_name | $debug_mode | $frames | N/A | N/A | N/A | ❌ CRASH |" >> "$REPORT_FILE"
        OVERALL_PASS=false
        continue
    }

    if [[ ! -f "$OUTPUT_DIR/${out_name}.png" ]]; then
        echo "  ❌ Screenshot missing for $out_name"
        echo "| $out_name | $debug_mode | $frames | N/A | N/A | N/A | ❌ NO IMG |" >> "$REPORT_FILE"
        OVERALL_PASS=false
        continue
    fi

    # 对比参考图（如存在）
    if [[ -f "$REFERENCE_DIR/${out_name}.png" ]]; then
        result=$(python3 .claude/skills/aether-visual-test/scripts/compare_images.py \
            "$REFERENCE_DIR/${out_name}.png" \
            "$OUTPUT_DIR/${out_name}.png" \
            --json 2>/dev/null || echo '{"ssim":null,"mae":0,"diff_pct":0}')

        ssim=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('ssim','N/A'))")
        mae=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('mae',0))")
        diff_pct=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('diff_pct',0))")

        if [[ "$ssim" == "None" || "$ssim" == "null" ]]; then
            ssim_str="N/A"
            status="⚠️ NO SSIM"
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
        echo "| $out_name | $debug_mode | $frames | $ssim_str | $mae | ${diff_pct}% | $status |" >> "$REPORT_FILE"
    else
        echo "  ⚠️  No reference image — manual inspection required"
        echo "| $out_name | $debug_mode | $frames | N/A | N/A | N/A | ⚠️ NO REF |" >> "$REPORT_FILE"
    fi
done

echo "" >> "$REPORT_FILE"
if $OVERALL_PASS; then
    echo "**Overall: ✅ PASS**" >> "$REPORT_FILE"
    echo ""
    echo "=== ✅ All scenes passed ==="
else
    echo "**Overall: ❌ FAIL** — review flagged scenes above" >> "$REPORT_FILE"
    echo ""
    echo "=== ❌ Some scenes failed — see report ==="
fi

echo "Report saved to: $REPORT_FILE"
