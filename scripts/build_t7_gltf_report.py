#!/usr/bin/env python3
"""Build the T7.1 owned-glTF fixture aggregate and preserve its evidence."""

import argparse
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import platform
import struct
import sys
import zlib


CASE_ID = "t7_gltf_document_fixture"
VARIANT_IDS = ["valid_128", "malformed_graph", "invalid_129"]
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


def esc(value):
    return html.escape(str(value), quote=True)


def typed_value(value):
    if isinstance(value, dict):
        return {key: value[key] for key in ("kind", "value") if key in value}
    return value


def compare_metric(operator, actual, expected):
    return {
        "Eq": lambda: actual == expected,
        "Ne": lambda: actual != expected,
        "Lt": lambda: actual < expected,
        "Le": lambda: actual <= expected,
        "Gt": lambda: actual > expected,
        "Ge": lambda: actual >= expected,
    }.get(operator, lambda: False)()


def check_variant(expected, actual):
    outcome = expected.get("expected_result", {})
    if outcome.get("kind") == "ExpectedError":
        passed = actual.get("kind") == "ExpectedError" and actual.get("code") == outcome.get("code")
        return [{"name": "expected_error", "expected": outcome.get("code"), "actual": actual.get("code"), "passed": passed}]

    checks = []
    checks.append({"name": "diagnostics", "expected": outcome.get("diagnostics", []), "actual": actual.get("diagnostics", []), "passed": outcome.get("diagnostics", []) == actual.get("diagnostics", [])})
    checks.append({"name": "fallbacks", "expected": outcome.get("fallbacks", []), "actual": actual.get("fallbacks", []), "passed": outcome.get("fallbacks", []) == actual.get("fallbacks", [])})
    for metric in outcome.get("metrics", []):
        observed = actual.get("metrics", {}).get(metric.get("name"))
        passed = observed is not None and compare_metric(metric.get("operator"), observed, metric.get("value"))
        checks.append({"name": f"{metric.get('name')} {metric.get('operator')}", "expected": metric.get("value"), "actual": observed, "passed": passed})
    actual_probes = {
        probe.get("name"): typed_value(probe.get("value"))
        for probe in actual.get("probes", [])
        if isinstance(probe, dict)
    }
    for probe in outcome.get("probes", []):
        name = probe.get("name", "")
        expected_value = typed_value(probe.get("value"))
        observed = actual_probes.get(name)
        checks.append({"name": f"{name} probe", "expected": expected_value, "actual": observed, "passed": observed == expected_value})
    return checks


def build(args):
    case_records = read_json(args.case_file, [])
    events = read_json(args.case_dir / "gltf-diagnostics.json", {})
    if not isinstance(case_records, list) or len(case_records) != 1 or case_records[0].get("id") != CASE_ID:
        raise ValueError("T7.1 requires the frozen glTF document VisualCase")
    case = case_records[0]
    expected_variants = case.get("variants", [])
    actual_variants = events.get("variants", [])
    if [item.get("id") for item in expected_variants] != VARIANT_IDS:
        raise ValueError("T7.1 VisualCase variant order changed")
    actual_by_id = {item.get("id"): item for item in actual_variants}
    checks = []
    for expected in expected_variants:
        actual = actual_by_id.get(expected.get("id"), {})
        checks.extend({"variant": expected["id"], **check} for check in check_variant(expected, actual))
    identity_passed = events.get("case_id") == CASE_ID and events.get("kind") == "Fixture"
    variants_passed = [item.get("id") for item in actual_variants] == VARIANT_IDS
    aggregate_passed = args.exit_code == 0 and identity_passed and variants_passed and bool(checks) and all(item["passed"] for item in checks)
    status = "PASS" if aggregate_passed else "FAIL"

    args.case_dir.mkdir(parents=True, exist_ok=True)
    for name in ("unit.stdout", "unit.stderr", "fixture.stdout", "fixture.stderr", "stdout", "stderr"):
        log_path = args.case_dir / name
        if log_path.is_file():
            normalized = log_path.read_text(encoding="utf-8", errors="replace").rstrip("\r\n")
            log_path.write_text(normalized + "\n" if normalized else "", encoding="utf-8")
    now = datetime.now(timezone.utc).isoformat()
    (args.case_dir / "output.png").write_bytes(sentinel_png())
    (args.case_dir / "reference.png").write_bytes(sentinel_png())
    (args.case_dir / "diff.png").write_bytes(sentinel_png())
    (args.case_dir / "metrics.json").write_text(json.dumps({
        "case_id": CASE_ID,
        "kind": "Fixture",
        "aggregate": status,
        "comparison": "not_applicable_cpu_fixture",
        "variant_count": len(actual_variants),
        "passed_check_count": sum(item["passed"] for item in checks),
        "check_count": len(checks),
    }, indent=2) + "\n", encoding="utf-8")
    (args.case_dir / "graph.txt").write_text("not_applicable: T7.1 validates CPU-owned glTF document data; no launcher or render graph was started\n", encoding="utf-8")
    (args.case_dir / "launcher.log").write_text(json.dumps({
        "case_id": CASE_ID,
        "run_id": args.run_id,
        "runner": "t7-gltf-slice-runner/1",
        "runner_pid": args.runner_pid,
        "command": args.command,
        "exit_code": args.exit_code,
        "started_at": args.started_at,
        "ended_at": now,
        "os": platform.platform(),
        "machine": platform.machine(),
        "launcher_started": False,
        "window_opened": False,
        "child_processes_started": False,
        "status": status,
    }, indent=2) + "\n", encoding="utf-8")

    rows = []
    for item in checks:
        row_status = "PASS" if item["passed"] else "FAIL"
        rows.append(
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>".format(
                row_status.lower(), esc(item.get("variant", "")), row_status,
                esc(json.dumps(item.get("expected"), sort_keys=True)),
                esc(json.dumps(item.get("actual"), sort_keys=True)),
            )
        )
    if not rows:
        rows.append('<tr class="fail"><td colspan="4">No acceptance checks were emitted</td></tr>')
    artifact_names = ["gltf-diagnostics.json", "metrics.json", "launcher.log", "graph.txt", "stdout", "stderr", "output.png", "reference.png", "diff.png"]
    links = " ".join(f'<a href="{name}">{esc(name)}</a>' for name in artifact_names)
    report = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T7.1 Owned glTF document — {status}</title>
<style>body{{font:15px/1.5 system-ui,sans-serif;max-width:1100px;margin:2rem auto;padding:0 1rem;color:#172033}}table{{border-collapse:collapse;width:100%;margin:1rem 0}}th,td{{text-align:left;padding:.65rem;border-bottom:1px solid #d0d5dd;vertical-align:top}}th{{background:#f2f4f7}}tr.pass td:nth-child(3){{color:#15803d;font-weight:700}}tr.fail td:nth-child(3){{color:#b42318;font-weight:700}}.status{{padding:.8rem 1rem;border-left:4px solid {'#15803d' if aggregate_passed else '#b42318'};background:{'#f0fdf4' if aggregate_passed else '#fef3f2'}}}code{{overflow-wrap:anywhere}}a{{margin-right:.7rem}}</style></head><body>
<h1>T7.1 — Owned glTF document and skeleton asset</h1><p class="status">Aggregate: <strong>{status}</strong> · CPU fixture only; no scene window was opened.</p>
<p>Run: <code>{esc(args.run_id)}</code> · Commit: <code>{esc(args.commit)}</code> · Fixture exit: <strong>{args.exit_code}</strong> · Case schema: <strong>{'valid' if identity_passed else 'invalid or missing'}</strong></p>
<h2>Variant checks</h2><table><thead><tr><th>Variant / check</th><th>Status</th><th>Expected</th><th>Observed</th></tr></thead><tbody>{''.join(rows)}</tbody></table>
<h2>Artifacts</h2><p>{links}</p><p>PNG files are 1×1 RGBA8 sentinels and are not rendered visual evidence. No launcher process was started.</p></body></html>"""
    (args.case_dir / "report.html").write_text(report, encoding="utf-8")
    (args.case_dir / "aggregate.json").write_text(json.dumps({
        "case_id": CASE_ID,
        "aggregate": status,
        "checks": checks,
        "fixture_exit_code": args.exit_code,
        "case_identity_passed": identity_passed,
    }, indent=2) + "\n", encoding="utf-8")
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
        return build(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"could not build T7.1 slice report: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
