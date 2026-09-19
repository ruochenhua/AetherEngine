#!/usr/bin/env bash
# T2 multi-light verification. Produces HTML only and does not launch Metal.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_NAME="${1:-$(date +%Y%m%d-%H%M%S)-t2-lighting}"
REPORT_DIR="tests/reports"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-t2.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

cases=(
  "t2_local_light_abi|t2_abi_tests"
  "t2_lighting_shader|lighting::pipeline::shader_tests"
  "t2_three_local_lights|renderer::lighting::tests"
  "t2_light_validation|local_light_validation_rejects_non_finite_or_non_positive_range"
  "t2_scene_parse|scene_description_supports_three_distinct_local_lights"
  "t2_scene_spawn|build_world_spawns_all_configured_local_lights"
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
    printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T2 case</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1000px;margin:32px auto;color:#172033}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}table{border-collapse:collapse;width:100%}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:12px;border-left:4px solid #2563eb}</style></head><body>'
    printf '<h1>%s</h1>\n' "$case_id"
    printf '<table><tr><th>状态</th><td class="%s">%s</td></tr><tr><th>测试</th><td><code>cargo test -p aether-engine %s --no-fail-fast</code></td></tr></table>\n' "$(echo "$status" | tr '[:upper:]' '[:lower:]')" "$status" "$test_filter"
    printf '%s\n' '<p class="note">这是 T2 多光源定向 fixture；不启动 launcher，不更新截图 reference。</p><h2>完整输出</h2><pre>'
    escape_html <"$output_file"
    printf '%s\n' '</pre></body></html>'
  } >"$case_report"
  case_rows+=("$case_id|$status|$case_report")
done

aggregate="$REPORT_DIR/${REPORT_NAME}.html"
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T2 lighting report</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1100px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.callout{background:#f0fdf4;padding:14px;border-left:4px solid #15803d}.failbox{background:#fef3f2;padding:14px;border-left:4px solid #b42318}a{color:#2563eb}</style></head><body>'
  printf '<h1>T2 · 多光源</h1><p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  if [ "$overall" -eq 0 ]; then
    printf '%s\n' '<div class="callout"><strong>PASS</strong>：T2 定向 fixture 全部通过。</div>'
  else
    printf '%s\n' '<div class="failbox"><strong>FAIL</strong>：至少一个 fixture 失败；各 case HTML 保留完整日志。</div>'
  fi
  printf '%s\n' '<h2>Primary cases</h2><table><tr><th>Case</th><th>Status</th><th>Report</th></tr>'
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report <<<"$row"
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s</td><td><a href="%s">HTML evidence</a></td></tr>\n' "$case_id" "$class" "$status" "$(basename "$case_report")"
  done
  printf '%s\n' '</table><h2>覆盖范围</h2><ul><li>固定 64B GpuLocalLight ABI</li><li>红/绿/蓝三局部光源提取、稳定排序和 32 上限</li><li>非法 range 的路径化错误</li><li>三局部光源 RON 场景解析与 ECS spawn</li></ul><p>渲染截图与 Metal launcher 冒烟应使用独立人工步骤执行。</p></body></html>'
} >"$aggregate"

echo "T2 report: $aggregate"
exit "$overall"
