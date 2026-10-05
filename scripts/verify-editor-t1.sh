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
  "t1_editor_transaction|failed_save_rolls_back_world_and_history,failed_undo_save_preserves_world_and_history"
  "t1_snapshot_whitelist|snapshot_is_schema_v2_and_excludes_runtime_entity_bits,duplicate_component_and_name_fail_without_partial_ecs_write"
  "t1_editor_command_replay|rename_persists_scene_and_operation_jsonl,copy_delete_component_add_and_replay_are_deterministic,delete_undo_redo_and_replay_preserve_state_hash,ten_copy_rename_pairs_replay_to_the_same_state_hash,state_sha256_is_stable_and_changes_with_world_state"
)

escape_html() {
  sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'
}

mkdir -p "$REPORT_DIR"
overall=0
case_rows=()

for item in "${cases[@]}"; do
  case_id="${item%%|*}"
  test_list="${item#*|}"
  output_file="$TMP_DIR/$case_id.log"
  status="PASS"
  IFS=',' read -r -a test_names <<< "$test_list"
  passed=0

  : >"$output_file"
  for test_name in "${test_names[@]}"; do
    test_filter="editor::tests::$test_name"
    if cargo test -p aether-engine --lib "$test_filter" -- --exact >>"$output_file" 2>&1; then
      if grep -Fq "test $test_filter ... ok" "$output_file"; then
        passed=$((passed + 1))
      else
        status="FAIL"
      fi
    else
      status="FAIL"
    fi
  done
  expected="${#test_names[@]}"
  if [ "$passed" -ne "$expected" ]; then
    status="FAIL"
    overall=1
  fi

  case_report="$REPORT_DIR/${REPORT_NAME}-${case_id}.html"
  {
    printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T1 case</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1000px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:12px;border-left:4px solid #2563eb}</style></head><body>'
    printf '<h1>%s</h1>\n' "$case_id"
    printf '<table><tr><th>状态</th><td class="%s">%s</td></tr><tr><th>测试数</th><td>%s / %s</td></tr><tr><th>测试</th><td><code>cargo test -p aether-engine --lib editor::tests::&lt;name&gt; -- --exact</code></td></tr></table>\n' "$(echo "$status" | tr '[:upper:]' '[:lower:]')" "$status" "$passed" "$expected"
    printf '%s\n' '<p class="note">这是 T1 editor CPU 单测分组，不是 VisualCase v2 fixture；不启动 launcher、不涉及 Metal，也不生成截图或修改视觉 reference。</p><h2>完整输出</h2><pre>'
    escape_html <"$output_file"
    printf '%s\n' '</pre></body></html>'
  } >"$case_report"

  case_rows+=("$case_id|$status|$case_report|$passed|$expected")
done

aggregate="$REPORT_DIR/${REPORT_NAME}.html"
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>Aether T1 editor report</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1100px;margin:32px auto;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.callout{background:#f0fdf4;padding:14px;border-left:4px solid #15803d}.failbox{background:#fef3f2;padding:14px;border-left:4px solid #b42318}a{color:#2563eb}</style></head><body>'
  printf '<h1>T1 · 编辑器事务与快照</h1><p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  if [ "$overall" -eq 0 ]; then
    printf '%s\n' '<div class="callout"><strong>PASS</strong>：三组 T1 核心单测全部通过。</div>'
  else
    printf '%s\n' '<div class="failbox"><strong>FAIL</strong>：至少一组 T1 核心单测失败；各分组 HTML 保留完整日志。</div>'
  fi
  printf '%s\n' '<h2>Test groups</h2><table><tr><th>Case</th><th>Status</th><th>Report</th></tr>'
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report passed expected <<<"$row"
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s (%s/%s)</td><td><a href="%s">HTML evidence</a></td></tr>\n' "$case_id" "$class" "$status" "$passed" "$expected" "$(basename "$case_report")"
  done
  printf '<tr><td colspan="3"><a href="%s">Machine-readable summary</a></td></tr>\n' "$(basename "$REPORT_NAME")-aggregate.json"
  printf '%s\n' '</table><h2>覆盖范围</h2><ul><li>world/file/undo 失败回滚</li><li>schema v2 whitelist 与 runtime Entity bits 隔离</li><li>copy/delete/rename、duplicate validation、JSONL replay 与 10 轮状态 hash</li></ul><p>本报告是 CPU 单测记录，不等同于 Revision 4 VisualCase v2 primary fixture，也不更新 reference。</p></body></html>'
} >"$aggregate"

aggregate_json="$REPORT_DIR/${REPORT_NAME}-aggregate.json"
{
  printf '{\n  "schema_version": 1,\n  "id": "t1_editor_cpu_suite",\n  "status": "%s",\n' "$([ "$overall" -eq 0 ] && printf 'pass' || printf 'fail')"
  printf '  "evidence_kind": "deterministic CPU unit tests; no renderer/GPU acceptance claimed",\n  "cases": [\n'
  first=1
  for row in "${case_rows[@]}"; do
    IFS='|' read -r case_id status case_report passed expected <<<"$row"
    [ "$first" -eq 1 ] || printf ',\n'
    first=0
    case_status="pass"
    [ "$status" = "PASS" ] || case_status="fail"
    printf '    {"id":"%s","status":"%s","tests":{"passed":%s,"expected":%s,"failed":%s},"report":"%s"}' \
      "$case_id" "$case_status" "$passed" "$expected" "$((expected - passed))" "$(basename "$case_report")"
  done
  printf '\n  ]\n}\n'
} >"$aggregate_json"

echo "T1 report: $aggregate"
echo "T1 aggregate: $aggregate_json"
exit "$overall"
