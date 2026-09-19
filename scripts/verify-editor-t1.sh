#!/usr/bin/env bash
# T1 editor transaction verification. Produces HTML only; no launcher or Metal.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_NAME="${1:-$(date +%Y%m%d-%H%M%S)-t1-editor}"
REPORT_DIR="tests/reports"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-t1.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

cases=(
  "t1_editor_transaction|editor::tests::failed_"
  "t1_snapshot_whitelist|editor::tests::snapshot_is_schema_v2_and_excludes_runtime_entity_bits"
  "t1_editor_command_replay|editor::tests::delete_undo_redo_and_replay_preserve_state_hash"
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
    printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T1 case</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1000px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:12px;border-left:4px solid #2563eb}</style></head><body>'
    printf '<h1>%s</h1>\n' "$case_id"
    printf '<table><tr><th>状态</th><td class="%s">%s</td></tr><tr><th>测试</th><td><code>cargo test -p aether-engine %s --no-fail-fast</code></td></tr></table>\n' "$(echo "$status" | tr '[:upper:]' '[:lower:]')" "$status" "$test_filter"
    printf '%s\n' '<p class="note">这是 T1 editor fixture case；不涉及 Metal、launcher、截图或视觉 reference。截图项对本任务不适用，保留测试输出作为唯一证据。</p><h2>完整输出</h2><pre>'
    escape_html <"$output_file"
    printf '%s\n' '</pre></body></html>'
  } >"$case_report"

  case_rows+=("$case_id|$status|$case_report")
done

aggregate="$REPORT_DIR/${REPORT_NAME}.html"
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T1 editor report</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1100px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.callout{background:#f0fdf4;padding:14px;border-left:4px solid #15803d}.failbox{background:#fef3f2;padding:14px;border-left:4px solid #b42318}a{color:#2563eb}</style></head><body>'
  printf '<h1>T1 · 编辑器事务与快照</h1><p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  if [ "$overall" -eq 0 ]; then
    printf '%s\n' '<div class="callout"><strong>PASS</strong>：三个 primary case 全部通过。</div>'
  else
    printf '%s\n' '<div class="failbox"><strong>FAIL</strong>：至少一个 primary case 失败；各 case HTML 保留完整日志。</div>'
  fi
  printf '%s\n' '<h2>Primary cases</h2><table><tr><th>Case</th><th>Status</th><th>Report</th></tr>'
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report <<<"$row"
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s</td><td><a href="%s">HTML evidence</a></td></tr>\n' "$case_id" "$class" "$status" "$(basename "$case_report")"
  done
  printf '%s\n' '</table><h2>覆盖范围</h2><ul><li>world/file/undo 失败回滚</li><li>schema v2 whitelist 与 runtime Entity bits 隔离</li><li>copy/delete/rename、duplicate validation、JSONL replay 与 state hash</li></ul><p>本报告不更新 reference，不启动全量矩阵。</p></body></html>'
} >"$aggregate"

echo "T1 report: $aggregate"
exit "$overall"
