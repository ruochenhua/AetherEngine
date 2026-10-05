#!/usr/bin/env bash
# T2.3 consumer, kill-switch, and editor integration checks. No scene window is launched.
set -euo pipefail

PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"
REPORT_NAME="${1:-$(date -u +%Y%m%dT%H%M%SZ)-t2-consumers}"
REPORT="tests/reports/${REPORT_NAME}-t2_lighting_consumers.html"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aether-t2-consumers.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

cases=(
  "lighting_frame_counts|aether-engine|renderer::lighting::tests::extraction"
  "local_light_capability_and_kill_switch|aether-engine|renderer::passes::lighting::local_lights::tests"
  "directional_only_shader|aether-engine|directional_only_shader_parses_without_local_light_bindings"
  "sun_consumers|aether-engine|pass_uses_light_direction_for_sun"
  "cloud_sun_consumer|aether-engine|cloud_pass_applies_frame_with_high_quality"
  "point_spot_editor_buttons|aether-launcher|app::ui::editor_operations::tests"
  "local_lights_cli|aether-launcher|local_lights_default_on_and_can_be_disabled"
)

escape_html() {
  sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

mkdir -p tests/reports
overall=0
{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>T2.3 lighting consumers</title><style>body{font:15px/1.6 system-ui;color:#172033;max-width:1100px;margin:32px auto;padding:0 16px}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#101828;color:#f2f4f7;padding:14px}.note{background:#eff6ff;padding:12px}</style><h1>T2.3 · LightingFrame consumers and local-light switch</h1>'
  printf '<p>Run: <code>%s</code></p>\n' "$REPORT_NAME"
  printf '%s\n' '<p class="note">This report contains CPU/GPU-device unit tests and UI-context tests. It does not claim a rendered-image review; release scene evidence is reported separately.</p><table><tr><th>Check</th><th>Status</th><th>Command and output</th></tr>'
  for item in "${cases[@]}"; do
    IFS='|' read -r case_id package test_filter <<<"$item"
    output_file="$TMP_DIR/$case_id.log"
    status=PASS
    if ! cargo test -p "$package" "$test_filter" >"$output_file" 2>&1; then
      status=FAIL
      overall=1
    fi
    class="$(echo "$status" | tr '[:upper:]' '[:lower:]')"
    printf '<tr><td><code>%s</code></td><td class="%s">%s</td><td><details><summary>cargo test -p %s %s</summary><pre>' \
      "$case_id" "$class" "$status" "$package" "$test_filter"
    escape_html <"$output_file"
    printf '%s\n' '</pre></details></td></tr>'
  done
  printf '%s\n' '</table>'
  if [[ $overall -eq 0 ]]; then
    printf '%s\n' '<p class="pass"><strong>PASS</strong>: consumer, feature-off, shader fallback, and Point/Spot UI checks passed.</p>'
  else
    printf '%s\n' '<p class="fail"><strong>FAIL</strong>: one or more checks failed; per-check output is retained in this report.</p>'
  fi
  printf '%s\n' '</html>'
} >"$REPORT"

echo "T2.3 consumer report: $REPORT"
echo "status=$([[ $overall -eq 0 ]] && echo PASS || echo FAIL)"
exit "$overall"
