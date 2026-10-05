#!/usr/bin/env python3
"""Build the T1 VisualCase v2 editor fixture aggregate and retain evidence."""

import argparse
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import platform
import struct
import sys
import zlib


SLICES = {
    "T1.1": ("t1_editor_transaction", "Editor transaction and rollback"),
    "T1.2": ("t1_snapshot_whitelist", "Closed snapshot whitelist"),
    "T1.3": ("t1_editor_command_replay", "Editor operation replay"),
}
RUNNER_VERSION = "t1-editor-fixture-runner/1"
SENTINEL_RGBA = bytes((24, 48, 72, 255))


def sentinel_png():
    def chunk(kind, payload):
        body = kind + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixels = zlib.compress(b"\x00" + SENTINEL_RGBA)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", pixels) + chunk(b"IEND", b"")


def read_json(path, fallback):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return fallback


def escape(value):
    return html.escape(str(value), quote=True)


def typed(value):
    return {key: value[key] for key in ("kind", "value") if key in value} if isinstance(value, dict) else value


def compare(operator, actual, expected):
    functions = {
        "Eq": lambda: actual == expected,
        "Ne": lambda: actual != expected,
        "Lt": lambda: actual is not None and actual < expected,
        "Le": lambda: actual is not None and actual <= expected,
        "Gt": lambda: actual is not None and actual > expected,
        "Ge": lambda: actual is not None and actual >= expected,
    }
    return functions.get(operator, lambda: False)()


def build(args):
    case_id, title = SLICES[args.slice]
    cases = read_json(args.case_file, [])
    if not isinstance(cases, list) or len(cases) != 1 or cases[0].get("id") != case_id:
        raise ValueError(f"{args.slice} requires exactly the frozen {case_id} VisualCase")
    case = cases[0]
    variants = case.get("variants", [])
    if len(variants) != 1 or variants[0].get("id") != "default":
        raise ValueError("T1 fixture must contain exactly one default variant")
    expected = variants[0].get("expected_result", {})
    if expected.get("kind") != "Render":
        raise ValueError("T1 fixture result must use the Render expectation envelope")

    args.case_dir.mkdir(parents=True, exist_ok=True)
    events = read_json(args.case_dir / "editor-events.json", {})
    identity = events.get("case_id") == case_id and events.get("kind") == "Fixture"
    actual_probes = {
        item.get("name"): typed(item.get("value"))
        for item in events.get("probes", [])
        if isinstance(item, dict)
    }
    ui_events = read_json(args.case_dir / "editor-ui-events.json", {})
    ui_identity = ui_events.get("case_id") == case_id and ui_events.get("kind") == "Fixture"
    apply_events = read_json(args.case_dir / "editor-apply-events.json", {})
    apply_identity = apply_events.get("case_id") == case_id and apply_events.get("kind") == "Fixture"
    for source in (ui_events, apply_events):
        actual_probes.update({
        item.get("name"): typed(item.get("value"))
        for item in source.get("probes", [])
        if isinstance(item, dict)
        })
    actual_metrics = events.get("metrics", {})
    checks = []
    for metric in expected.get("metrics", []):
        name = metric["name"]
        actual = actual_metrics.get(name)
        checks.append({
            "name": f"{name} {metric['operator']}",
            "expected": metric["value"],
            "actual": actual,
            "passed": compare(metric["operator"], actual, metric["value"]),
        })
    for probe in expected.get("probes", []):
        name = probe["name"]
        observed = actual_probes.get(name)
        wanted = typed(probe["value"])
        checks.append({"name": name, "expected": wanted, "actual": observed, "passed": observed == wanted})

    primary_passed = args.exit_code == 0 and identity and bool(checks) and all(item["passed"] for item in checks)
    ui_smoke = args.slice != "T1.3" or (
        ui_identity
        and apply_identity
        and args.ui_exit_code == 0
        and args.apply_exit_code == 0
        and actual_probes.get("ui_event_to_operation") == {"kind": "Bool", "value": True}
        and actual_probes.get("queued_operation_persisted") == {"kind": "Bool", "value": True}
        and actual_probes.get("undo_updates_replay_log") == {"kind": "Bool", "value": True}
        and actual_probes.get("redo_updates_replay_log") == {"kind": "Bool", "value": True}
    )
    slice_passed = primary_passed and ui_smoke
    status = "PASS" if primary_passed else "FAIL"
    acceptance_status = "PASS" if slice_passed else "OPEN"
    now = datetime.now(timezone.utc).isoformat()
    sentinel = sentinel_png()
    for name in ("output.png", "reference.png", "diff.png"):
        (args.case_dir / name).write_bytes(sentinel)
    (args.case_dir / "metrics.json").write_text(json.dumps({
        "case_id": case_id,
        "slice": args.slice,
        "aggregate": status,
        "slice_acceptance": acceptance_status,
        "comparison": "not_applicable_cpu_fixture",
        "sentinel": {"width": 1, "height": 1, "format": "png-rgba8", "rgba": list(SENTINEL_RGBA)},
        "checks": len(checks),
        "passed_checks": sum(item["passed"] for item in checks),
        "ui_event_to_operation_smoke": "pass" if args.slice == "T1.3" and ui_smoke else "not_applicable" if ui_smoke else "missing",
        "ui_exit_code": args.ui_exit_code,
        "apply_exit_code": args.apply_exit_code,
    }, indent=2) + "\n", encoding="utf-8")
    (args.case_dir / "graph.txt").write_text(
        "not_applicable: CPU editor fixture; no launcher, renderer, GPU, or render graph was started\n",
        encoding="utf-8",
    )
    (args.case_dir / "launcher.log").write_text(json.dumps({
        "case_id": case_id,
        "slice": args.slice,
        "run_id": args.run_id,
        "runner": RUNNER_VERSION,
        "runner_pid": args.runner_pid,
        "command": args.command,
        "exit_code": args.exit_code,
        "ui_exit_code": args.ui_exit_code,
        "started_at": args.started_at,
        "ended_at": now,
        "os": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "launcher_started": False,
        "window_opened": False,
        "child_processes_started": False,
        "status": status,
        "slice_acceptance": acceptance_status,
    }, indent=2) + "\n", encoding="utf-8")

    rows = []
    for check in checks:
        check_status = "PASS" if check["passed"] else "FAIL"
        rows.append(
            '<tr class="{}"><td>{}</td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>'.format(
                check_status.lower(), escape(check["name"]), check_status,
                escape(json.dumps(check["expected"], sort_keys=True)),
                escape(json.dumps(check["actual"], sort_keys=True)),
            )
        )
    if not rows:
        rows.append('<tr class="fail"><td colspan="4">No fixture checks emitted</td></tr>')
    artifact_names = ["editor-events.json", "metrics.json", "launcher.log", "graph.txt", "stdout", "stderr", "output.png", "reference.png", "diff.png"]
    if args.slice == "T1.3":
        artifact_names.extend(("editor-ui-events.json", "editor-apply-events.json", "ui.stdout", "ui.stderr", "apply.stdout", "apply.stderr"))
    links = " ".join(f'<a href="{name}">{escape(name)}</a>' for name in artifact_names)
    (args.case_dir / "report.html").write_text(f"""<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{escape(args.slice)} {escape(title)} — {status}</title>
<style>body{{font:15px/1.6 system-ui,sans-serif;max-width:1080px;margin:2rem auto;padding:0 1rem;color:#172033}}table{{border-collapse:collapse;width:100%;margin:1rem 0}}th,td{{text-align:left;padding:.65rem;border-bottom:1px solid #d0d5dd;vertical-align:top}}th{{background:#f2f4f7}}.pass td:nth-child(2){{color:#15803d;font-weight:700}}.fail td:nth-child(2){{color:#b42318;font-weight:700}}.status{{padding:.8rem 1rem;border-left:4px solid {'#15803d' if slice_passed else '#b42318'};background:{'#f0fdf4' if slice_passed else '#fef3f2'}}}code{{overflow-wrap:anywhere}}a{{margin-right:.7rem}}</style></head><body>
<h1>{escape(args.slice)} — {escape(title)}</h1>
<p class="status">Primary fixture: <strong>{status}</strong> · Slice acceptance: <strong>{acceptance_status}</strong></p>
<p>Run: <code>{escape(args.run_id)}</code> · Commit: <code>{escape(args.commit)}</code> · Started: <code>{escape(args.started_at)}</code> · Completed: <code>{escape(now)}</code></p>
<p>Fixture exit: <strong>{args.exit_code}</strong> · Case schema/events: <strong>{'valid' if identity else 'missing or invalid'}</strong> · Launcher/window: <strong>not started</strong></p>
<h2>Typed fixture checks</h2><table><thead><tr><th>Check</th><th>Status</th><th>Expected</th><th>Observed</th></tr></thead><tbody>{''.join(rows)}</tbody></table>
<h2>Artifacts</h2><p>{links}</p>
<p>The three PNGs are 1×1 RGBA8 sentinels required by the fixture report contract. They are not rendered visual evidence. No golden comparison was run.</p>
{'<p><strong>T1.3 remains open:</strong> the Launcher UI event-to-operation smoke failed or emitted no typed evidence.</p>' if not ui_smoke else ''}
</body></html>""", encoding="utf-8")
    summary = {
        "case_id": case_id,
        "slice": args.slice,
        "aggregate": status,
        "slice_acceptance": acceptance_status,
        "case_identity_passed": identity,
        "checks": checks,
        "command_exit_code": args.exit_code,
        "launcher_started": False,
        "ui_event_to_operation_smoke": "pass" if args.slice == "T1.3" and ui_smoke else "not_applicable" if ui_smoke else "missing",
        "ui_exit_code": args.ui_exit_code,
        "apply_exit_code": args.apply_exit_code,
    }
    (args.case_dir / "aggregate.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    return 0 if primary_passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case-file", type=Path, required=True)
    parser.add_argument("--case-dir", type=Path, required=True)
    parser.add_argument("--slice", choices=sorted(SLICES), required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--command", required=True)
    parser.add_argument("--runner-pid", type=int, required=True)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--exit-code", type=int, required=True)
    parser.add_argument("--ui-exit-code", type=int, default=0)
    parser.add_argument("--apply-exit-code", type=int, default=0)
    args = parser.parse_args()
    try:
        return build(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"could not build T1 editor report: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
