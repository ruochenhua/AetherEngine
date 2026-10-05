#!/usr/bin/env python3
"""Render an auditable HTML summary for one T9.1 benchmark capture."""

from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
from typing import Any


def escaped(value: Any) -> str:
    return html.escape(str(value))


def metric(value: Any, unit: str = "") -> str:
    if value is None:
        return "Unavailable"
    return f"{float(value):.3f}{unit}"


def cpu_plot(samples: list[dict[str, Any]]) -> str:
    measured = [
        sample["cpu_extract_ms"]
        for sample in samples
        if sample["status"]["kind"] == "Valid" and sample["cpu_extract_ms"] is not None
    ]
    if not measured:
        return "<p>CPU timing samples are unavailable.</p>"
    width, height, padding = 900, 180, 8
    maximum = max(max(measured), 0.001)
    points = []
    for index, value in enumerate(measured):
        x = padding + index * (width - 2 * padding) / max(len(measured) - 1, 1)
        y = height - padding - value * (height - 2 * padding) / maximum
        points.append(f"{x:.1f},{y:.1f}")
    return (
        f'<svg viewBox="0 0 {width} {height}" role="img" '
        'aria-label="CPU extraction time by sampled frame">'
        f'<polyline points="{" ".join(points)}" fill="none" stroke="#2563eb" '
        'stroke-width="2"/></svg>'
    )


def render_report(series: dict[str, Any], case: dict[str, Any], image_exists: bool) -> str:
    expected = case["samples"]
    actual = len(series["samples"])
    complete = series["status"] == "Complete" and actual == expected
    baseline_matches = series["commit"] == case["baseline_commit"] and not series["source_tree_dirty"]
    warning = ""
    if not baseline_matches:
        warning = (
            '<section class="notice"><strong>Source boundary:</strong> this is a complete capture '
            "of the measured source state, but it does not represent the clean pinned baseline. "
            f'Pinned baseline {escaped(case["baseline_commit"])}; measured commit '
            f'{escaped(series["commit"])}; working tree dirty '
            f'{escaped(series["source_tree_dirty"])}. Keep this report as a current-source '
            "profile, not as the v0.1 release baseline.</section>"
        )
    gpu = (
        f'{metric(series["p50_gpu_ms"], " ms")} / {metric(series["p95_gpu_ms"], " ms")}'
        if series["gpu_timer"] != "Unavailable"
        else "Unavailable (null; timestamp queries were not enabled)"
    )
    rows = "".join(
        "<tr>"
        f"<td>{sample['frame_index']}</td>"
        f"<td>{escaped(sample['status']['kind'])}</td>"
        f"<td>{metric(sample['cpu_extract_ms'], ' ms')}</td>"
        f"<td>{sample['draw_calls']}</td>"
        f"<td>{sample['visible_entities']}</td>"
        f"<td>{metric(sample['gpu_pass_ms'], ' ms')}</td>"
        "</tr>"
        for sample in series["samples"]
    )
    image = (
        '<img class="capture" src="output.png" alt="T9.1 benchmark scene">'
        if image_exists
        else "<p>Scene capture was not produced.</p>"
    )
    status = "PASS" if complete else "INCOMPLETE"
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T9.1 baseline profile — {status}</title>
<style>
body{{font:15px/1.55 system-ui,sans-serif;max-width:1120px;margin:2rem auto;padding:0 1rem;color:#172033;background:#f8fafc}}
h1,h2{{letter-spacing:-.02em}} .status,.notice{{padding:1rem 1.2rem;border-radius:10px;margin:1rem 0}}
.status{{border-left:5px solid {'#16803c' if complete else '#b42318'};background:{'#effaf2' if complete else '#fef3f2'}}}
.notice{{border-left:5px solid #b54708;background:#fffaeb}} table{{border-collapse:collapse;width:100%;background:white}}
th,td{{text-align:left;padding:.55rem .65rem;border-bottom:1px solid #e4e7ec}}th{{background:#f2f4f7}}
svg{{width:100%;height:180px;background:white;border:1px solid #e4e7ec;border-radius:8px}}
.capture{{width:100%;height:auto;border:1px solid #cbd5e1;background:#111}}code{{overflow-wrap:anywhere}}
</style></head><body>
<h1>T9.1 · Performance baseline profile</h1>
<p class="status"><strong>{status}</strong> · Series status {escaped(series["status"])} ·
{actual} measured samples / {expected} requested after {series["warmup_frames"]} warm-up frames.</p>
{warning}
<h2>Summary</h2>
<table><thead><tr><th>Metric</th><th>p50</th><th>p95</th><th>Source</th></tr></thead><tbody>
<tr><td>CPU extraction</td><td>{metric(series["p50_cpu_ms"], " ms")}</td><td>{metric(series["p95_cpu_ms"], " ms")}</td><td>{escaped(series["cpu_timer"])}</td></tr>
<tr><td>GPU pass</td><td colspan="2">{gpu}</td><td>{escaped(series["gpu_timer"])}</td></tr>
<tr><td>Draw calls</td><td>{metric(series["p50_draw_calls"])}</td><td>—</td><td>instrumented renderer draw sites</td></tr>
<tr><td>Memory</td><td colspan="2">Unavailable (null)</td><td>no allocator or adapter memory counter</td></tr>
</tbody></table>
<h2>CPU extraction samples</h2>{cpu_plot(series["samples"])}
<h2>Scene capture</h2>{image}
<h2>Reproduction metadata</h2>
<table><tbody>
<tr><th>Case</th><td>{escaped(series["case_id"])}</td></tr>
<tr><th>Scene / fixture</th><td>{escaped(series["scene"])} / {escaped(series["fixture_source"])}</td></tr>
<tr><th>Entities / seed</th><td>{series["entity_count"]} / {series["seed"]}</td></tr>
<tr><th>Feature flags</th><td><code>{escaped(json.dumps(series["feature_flags"], sort_keys=True))}</code></td></tr>
<tr><th>Device</th><td>{escaped(series["device"])} · {escaped(series["adapter"])} · {escaped(series["driver"])}</td></tr>
<tr><th>Device key</th><td><code>{escaped(series["device_key"])}</code></td></tr>
<tr><th>Resolution</th><td>{series["resolution"][0]} × {series["resolution"][1]}</td></tr>
<tr><th>Measured commit</th><td><code>{escaped(series["commit"])}</code></td></tr>
<tr><th>Source fingerprint</th><td><code>{escaped(series["source_fingerprint"])}</code> · dirty={escaped(series["source_tree_dirty"])}</td></tr>
<tr><th>Pinned baseline commit</th><td><code>{escaped(series["baseline_commit"])}</code></td></tr>
<tr><th>RGBA8 SHA-256</th><td><code>{escaped(series["rgba8_sha256"])}</code></td></tr>
<tr><th>Visible entity SHA-256</th><td><code>{escaped(series["visible_entity_sha256"])}</code></td></tr>
</tbody></table>
<details><summary>All per-frame samples</summary><table><thead><tr><th>Frame</th><th>Status</th><th>CPU ms</th><th>Draw calls</th><th>Visible entities</th><th>GPU ms</th></tr></thead><tbody>{rows}</tbody></table></details>
<p><a href="benchmark.json">benchmark.json</a> · <a href="benchmark-case.json">benchmark-case.json</a> ·
<a href="launcher.stdout">launcher.stdout</a> · <a href="launcher.stderr">launcher.stderr</a></p>
</body></html>"""


def build(report_dir: Path) -> int:
    series_path = report_dir / "benchmark.json"
    case_path = report_dir / "benchmark-case.json"
    if not series_path.is_file() or not case_path.is_file():
        return 2
    series = json.loads(series_path.read_text(encoding="utf-8"))
    case = json.loads(case_path.read_text(encoding="utf-8"))
    page = render_report(series, case, (report_dir / "output.png").is_file())
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    status = "PASS" if series.get("status") == "Complete" and len(series.get("samples", [])) == case["samples"] else "INCOMPLETE"
    (report_dir / "report-status.txt").write_text(status + "\n", encoding="utf-8")
    return 0 if status == "PASS" else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", required=True, type=Path)
    args = parser.parse_args()
    return build(args.report_dir)


if __name__ == "__main__":
    raise SystemExit(main())
