#!/usr/bin/env bash
# T2.2 GPU ABI, bind-group contract, and low-limit shader verification.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_NAME="${1:-$(date -u +%Y%m%dT%H%M%SZ)-t2-abi}"
REPORT_DIR="tests/reports"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-t2-abi.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

cases=(
  "local_light_abi|t2_abi_tests"
  "local_light_device_capability|renderer::passes::lighting::local_lights::tests"
  "local_light_shader_and_fallback|renderer::passes::lighting::pipeline::shader_tests"
)

escape_html() {
  sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'
}

mkdir -p "$REPORT_DIR"
overall=0
case_rows=()

for item in "${cases[@]}"; do
  case_id="${item%%|*}"
  test_filter="${item#*|}"
  output_file="$TMP_DIR/$case_id.log"
  status="PASS"

  if ! cargo test -p aether-engine "$test_filter" --no-fail-fast >"$output_file" 2>&1; then
    status="FAIL"
    overall=1
  fi

  case_report="$REPORT_DIR/${REPORT_NAME}-${case_id}.html"
  {
    printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T2.2 ABI case</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1000px;margin:32px auto;color:#172033}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}table{border-collapse:collapse;width:100%}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:12px;border-left:4px solid #2563eb}</style></head><body>'
    printf '<h1>%s</h1>\n' "$case_id"
    printf '<table><tr><th>状态</th><td class="%s">%s</td></tr><tr><th>测试筛选</th><td><code>%s</code></td></tr></table>\n' "$(echo "$status" | tr '[:upper:]' '[:lower:]')" "$status" "$test_filter"
    printf '%s\n' '<p class="note">验证 Rust/WGSL 64B stride 与 16B 对齐、16B params、group 4 binding，以及低设备限制时无局部光的方向光 shader 变体。</p><h2>完整输出</h2><pre>'
    escape_html <"$output_file"
    printf '%s\n' '</pre></body></html>'
  } >"$case_report"
  case_rows+=("$case_id|$status|$case_report")
done

aggregate="$REPORT_DIR/${REPORT_NAME}.html"
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T2.2 GPU ABI report</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1100px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.callout{background:#f0fdf4;padding:14px;border-left:4px solid #15803b}.failbox{background:#fef3f2;padding:14px;border-left:4px solid #b42318}a{color:#2563eb}</style></head><body>'
  printf '<h1>T2.2 · 局部光 GPU ABI</h1><p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  if [ "$overall" -eq 0 ]; then
    printf '%s\n' '<div class="callout"><strong>PASS</strong>：ABI、group 4 和 direction-only shader 测试全部通过。</div>'
  else
    printf '%s\n' '<div class="failbox"><strong>FAIL</strong>：至少一个合同测试失败；逐项报告保留完整日志。</div>'
  fi
  printf '%s\n' '<h2>Primary cases</h2><table><tr><th>Case</th><th>Status</th><th>Report</th></tr>'
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report <<<"$row"
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s</td><td><a href="%s">HTML evidence</a></td></tr>\n' "$case_id" "$class" "$status" "$(basename "$case_report")"
  done
  printf '%s\n' '</table><p>此报告证明 ABI 和 CPU shader 合同；实际 pipeline 创建与场景画面另由 T2 三局部光 Release 场景运行验收。</p></body></html>'
} >"$aggregate"

echo "T2.2 report: $aggregate"
exit "$overall"
