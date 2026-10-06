#!/usr/bin/env bash
# T8.1 CPU lifecycle aggregate. It records test evidence without claiming a renderer run.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

RUN_ID="${1:-$(date +%Y%m%d-%H%M%S)-t8.1-asset-store}"
REPORT_DIR="tests/reports/$RUN_ID/t8_asset_store_lifecycle"
mkdir -p "$REPORT_DIR"

status="PASS"
if ! cargo test -p aether-engine --lib asset::store_tests >"$REPORT_DIR/stdout.log" 2>"$REPORT_DIR/stderr.log"; then
  status="FAIL"
fi

escape_html() {
  sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

status_class="pass"
if [ "$status" = "FAIL" ]; then
  status_class="fail"
fi

{
  printf '%s\n' '<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>T8.1 AssetStore lifecycle</title><style>body{font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;max-width:1050px;margin:32px auto;padding:0 20px;color:#172033}table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:9px;text-align:left}.pass{color:#15803d}.fail{color:#b42318}.note{background:#eff6ff;padding:14px;border-left:4px solid #2563eb}pre{background:#101828;color:#f2f4f7;padding:16px;overflow:auto;white-space:pre-wrap}</style></head><body>'
  printf '<h1>T8.1 · Typed AssetStore lifecycle</h1><p>Run: <code>%s</code></p>\n' "$RUN_ID"
  printf '<table><tr><th>Status</th><td class="%s">%s</td></tr><tr><th>Command</th><td><code>cargo test -p aether-engine --lib asset::store_tests</code></td></tr></table>\n' "$status_class" "$status"
  printf '%s\n' '<p class="note">CPU fixture aggregate. It covers canonical identity, typed payloads, queued and in-flight cooperative cancellation, bounded polling, reload ordering, last-known-good retention, release, and bounded shutdown. It does not claim a launcher render or a GPU upload.</p>'
  printf '%s\n' '<h2>Test output</h2><pre>'
  escape_html <"$REPORT_DIR/stdout.log"
  escape_html <"$REPORT_DIR/stderr.log"
  printf '%s\n' '</pre><h2>Artifacts</h2><ul><li><a href="stdout.log">stdout.log</a></li><li><a href="stderr.log">stderr.log</a></li></ul></body></html>'
} >"$REPORT_DIR/report.html"

printf 'T8.1 report: %s/report.html\n' "$REPORT_DIR"
[ "$status" = "PASS" ]
