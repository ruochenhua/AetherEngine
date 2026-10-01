#!/usr/bin/env python3
"""Build the T8.1 fixture aggregate from its VisualCase and test evidence."""

import argparse
from datetime import datetime, timezone
import html
import json
import os
from pathlib import Path
import platform
import struct
import sys
import zlib


RUNNER_VERSION = "t8-slice-runner/1"
SENTINEL_RGBA = bytes((24, 48, 72, 255))


def png_chunk(kind, payload):
    body = kind + payload
    return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def sentinel_png():
    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixels = zlib.compress(b"\x00" + SENTINEL_RGBA)
    return b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", header) + png_chunk(b"IDAT", pixels) + png_chunk(b"IEND", b"")


def read_json(path, fallback):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return fallback


def typed_value(value):
    if not isinstance(value, dict):
        return value
    return {key: value[key] for key in ("kind", "value") if key in value}


def escape(value):
    return html.escape(str(value), quote=True)


def build_report(args):
    case_records = read_json(args.case_file, [])
    if not isinstance(case_records, list) or len(case_records) != 1:
        raise ValueError("T8.1 case file must contain exactly one VisualCase v2 record")
    case = case_records[0]
    if case.get("id") != "t8_asset_store_lifecycle":
        raise ValueError("unexpected primary case id")
    variants = case.get("variants")
    if not isinstance(variants, list) or len(variants) != 1 or variants[0].get("id") != "default":
        raise ValueError("T8.1 requires exactly one default variant")
    expected_result = variants[0].get("expected_result", {})
    if expected_result.get("kind") != "Render":
        raise ValueError("T8.1 default variant must use the Render expectation")

    args.case_dir.mkdir(parents=True, exist_ok=True)
    events_path = args.case_dir / "asset-events.json"
    events = read_json(events_path, {"case_id": case["id"], "kind": "Fixture", "events": [], "probes": []})
    if not isinstance(events, dict):
        events = {"case_id": case["id"], "kind": "Fixture", "events": [], "probes": []}
    actual_probes = {
        probe.get("name"): typed_value(probe.get("value"))
        for probe in events.get("probes", [])
        if isinstance(probe, dict)
    }

    checks = []
    for expected in expected_result.get("probes", []):
        name = expected.get("name", "")
        expected_value = typed_value(expected.get("value"))
        actual_value = actual_probes.get(name)
        checks.append({"name": name, "expected": expected_value, "actual": actual_value, "passed": actual_value == expected_value})

    command_passed = args.exit_code == 0
    case_identity_passed = events.get("case_id") == case["id"] and events.get("kind") == "Fixture"
    aggregate_passed = command_passed and case_identity_passed and bool(checks) and all(item["passed"] for item in checks)
    status = "PASS" if aggregate_passed else "FAIL"
    now = datetime.now(timezone.utc).isoformat()
    sentinel = sentinel_png()
    for name in ("output.png", "reference.png", "diff.png"):
        (args.case_dir / name).write_bytes(sentinel)

    metrics = {
        "case_id": case["id"],
        "kind": "Fixture",
        "aggregate": status,
        "comparison": "not_applicable",
        "sentinel": {"width": 1, "height": 1, "format": "png-rgba8", "rgba": list(SENTINEL_RGBA)},
        "probe_count": len(checks),
        "passed_probe_count": sum(item["passed"] for item in checks),
    }
    (args.case_dir / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    (args.case_dir / "graph.txt").write_text(
        "not_applicable: CPU AssetStore fixture; no render graph was created\n", encoding="utf-8"
    )
    launcher_log = {
        "case_id": case["id"],
        "run_id": args.run_id,
        "runner": RUNNER_VERSION,
        "runner_pid": args.runner_pid,
        "command": args.command,
        "exit_code": args.exit_code,
        "started_at": args.started_at,
        "ended_at": now,
        "os": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "launcher_started": False,
        "window_opened": False,
        "status": status,
    }
    (args.case_dir / "launcher.log").write_text(json.dumps(launcher_log, indent=2) + "\n", encoding="utf-8")

    rows = []
    for check in checks:
        row_status = "PASS" if check["passed"] else "FAIL"
        rows.append(
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>".format(
                row_status.lower(), escape(check["name"]), row_status,
                escape(json.dumps(check["expected"], sort_keys=True)),
                escape(json.dumps(check["actual"], sort_keys=True)),
            )
        )
    if not rows:
        rows.append('<tr class="fail"><td colspan="4">No probe evidence emitted</td></tr>')

    artifact_links = " ".join(
        f'<a href="{name}">{escape(name)}</a>'
        for name in ("asset-events.json", "metrics.json", "launcher.log", "graph.txt", "stdout", "stderr", "output.png", "reference.png", "diff.png")
    )
    report = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T8.1 AssetStore Lifecycle — {status}</title>
<style>
body{{font:15px/1.5 system-ui,sans-serif;max-width:1100px;margin:2rem auto;padding:0 1rem;color:#172033}}
table{{border-collapse:collapse;width:100%;margin:1rem 0}}th,td{{text-align:left;padding:.65rem;border-bottom:1px solid #d0d5dd;vertical-align:top}}
th{{background:#f2f4f7}}tr.pass td:nth-child(2){{color:#15803d;font-weight:700}}tr.fail td:nth-child(2){{color:#b42318;font-weight:700}}
.status{{padding:.8rem 1rem;border-left:4px solid {'#15803d' if aggregate_passed else '#b42318'};background:{'#f0fdf4' if aggregate_passed else '#fef3f2'}}}
code{{overflow-wrap:anywhere}}a{{margin-right:.7rem}}
</style></head><body>
<h1>T8.1 — Typed AssetStore lifecycle</h1>
<p class="status">Aggregate: <strong>{status}</strong> · Fixture case · Visual comparison not applicable</p>
<p>Run: <code>{escape(args.run_id)}</code> · Started: <code>{escape(args.started_at)}</code> · Completed: <code>{escape(now)}</code></p>
<p>Environment: <code>{escape(platform.platform())}</code> · Python <code>{escape(platform.python_version())}</code> · Commit <code>{escape(args.commit)}</code></p>
<p>Command exit: <strong>{'0' if command_passed else escape(args.exit_code)}</strong> · Case schema: <strong>{'valid' if case_identity_passed else 'invalid or missing'}</strong></p>
<h2>Typed probes</h2><table><thead><tr><th>Probe</th><th>Status</th><th>Expected</th><th>Observed</th></tr></thead><tbody>{''.join(rows)}</tbody></table>
<h2>Lifecycle events</h2><ul>{''.join(f'<li>{escape(item)}</li>' for item in events.get('events', [])) or '<li>No lifecycle events emitted</li>'}</ul>
<h2>Artifacts</h2><p>{artifact_links}</p>
<p>PNG files are deterministic 1×1 RGBA8 sentinels for fixture-runner compatibility; they are not rendered visual evidence.</p>
</body></html>
"""
    (args.case_dir / "report.html").write_text(report, encoding="utf-8")
    summary = {"case_id": case["id"], "aggregate": status, "checks": checks, "command_exit_code": args.exit_code}
    (args.case_dir / "aggregate.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    return 0 if aggregate_passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case-file", type=Path, required=True)
    parser.add_argument("--case-dir", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--command", required=True)
    parser.add_argument("--runner-pid", type=int, required=True)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--exit-code", type=int, required=True)
    args = parser.parse_args()
    try:
        return build_report(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"could not build T8.1 report: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
