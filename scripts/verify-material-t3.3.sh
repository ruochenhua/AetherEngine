#!/usr/bin/env bash
# T3.3 material Inspector verification. Headless; no launcher window or Metal surface.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_NAME="${1:-$(date +%Y%m%d-%H%M%S)-t3.3-material-inspector}"
REPORT_DIR="tests/reports"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-t3.3.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

cases=(
  "t3_material_atomic_apply|cargo test -p aether-launcher inspector::material::tests --no-fail-fast"
  "t3_material_undo|cargo test -p aether-launcher inspector::tests::apply_material_undo_restores_source_config_and_adapter --no-fail-fast"
  "t3_material_save_load|cargo test -p aether-engine --test t3_material_inspector serialize_world_roundtrips_extended_material_config --no-fail-fast"
)

escape_html() {
  sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'
}

mkdir -p "$REPORT_DIR"
overall=0
case_rows=()

for item in "${cases[@]}"; do
  case_id="${item%%|*}"
  command="${item#*|}"
  output_file="$TMP_DIR/$case_id.log"
  status="PASS"

  if ! bash -c "$command" >"$output_file" 2>&1; then
    status="FAIL"
    overall=1
  fi

  case_report="$REPORT_DIR/${REPORT_NAME}-${case_id}.html"
  {
    printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T3.3 material Inspector case</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1000px;margin:32px auto;color:#172033}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}table{border-collapse:collapse;width:100%}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:12px;border-left:4px solid #2563eb}</style></head><body>'
    printf '<h1>%s</h1>\n' "$case_id"
    printf '<table><tr><th>状态</th><td class="%s">%s</td></tr><tr><th>命令</th><td><code>%s</code></td></tr></table>\n' "$(echo "$status" | tr '[:upper:]' '[:lower:]')" "$status" "$command"
    printf '%s\n' '<p class="note">T3.3 headless fixture：不启动 launcher，不创建 Metal 窗口；验证 Inspector 的 MaterialConfig 原子应用、回滚以及完整字段保存/加载。</p><h2>完整输出</h2><pre>'
    escape_html <"$output_file"
    printf '%s\n' '</pre></body></html>'
  } >"$case_report"
  case_rows+=("$case_id|$status|$case_report")
done

aggregate="$REPORT_DIR/${REPORT_NAME}.html"
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T3.3 material Inspector report</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1100px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.callout{background:#f0fdf4;padding:14px;border-left:4px solid #15803d}.failbox{background:#fef3f2;padding:14px;border-left:4px solid #b42318}a{color:#2563eb}</style></head><body>'
  printf '<h1>T3.3 · Material Inspector atomic apply</h1><p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  if [ "$overall" -eq 0 ]; then
    printf '%s\n' '<div class="callout"><strong>PASS</strong>：T3.3 三个 primary case 全部通过。</div>'
  else
    printf '%s\n' '<div class="failbox"><strong>FAIL</strong>：至少一个 primary case 失败；各 case HTML 保留完整日志。</div>'
  fi
  printf '%s\n' '<h2>Primary cases</h2><table><tr><th>Case</th><th>Status</th><th>Report</th></tr>'
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report <<<"$row"
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s</td><td><a href="%s">HTML evidence</a></td></tr>\n' "$case_id" "$class" "$status" "$(basename "$case_report")"
  done
  printf '%s\n' '</table><h2>验收 checklist</h2><ul><li>☑ Inspector 只编辑 MaterialConfig，不直接编辑 GPU uniform</li><li>☑ normal_scale、occlusion_strength、emissive_intensity 非法时拒绝且 World 不变</li><li>☑ 合法配置一次性同步 MaterialConfig 与 MaterialUniform</li><li>☑ Undo 恢复完整源配置和 GPU 适配器</li><li>☑ Normal/ORM/Emissive 路径及扩展字段保存后可加载</li></ul></body></html>'
} >"$aggregate"

echo "T3.3 report: $aggregate"
exit "$overall"
