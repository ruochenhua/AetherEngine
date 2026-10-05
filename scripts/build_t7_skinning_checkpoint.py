#!/usr/bin/env python3
"""Build the honest T7.2 palette-core review report."""

import argparse
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import platform
import re
import sys


CASE_ID = "t7_skinned_shadow_reflection"
EXPECTED_TESTS = [
    "gpu_palette_has_fixed_128_matrix_column_major_abi",
    "stale_generation_rejects_publication_without_advancing_frame_id",
    "primitive_bounds_reject_out_of_range_joint_indices",
    "palette_includes_non_joint_ancestor_transform",
    "extracted_palette_and_bounds_share_the_current_pose",
    "item_fallback_does_not_drop_other_primitive_outputs",
    "non_finite_pose_falls_back_only_for_the_affected_item",
    "storage_layout_binds_and_uploads_a_palette",
    "unsupported_129_joint_item_falls_back_without_a_skeleton_asset",
]


def esc(value):
    return html.escape(str(value), quote=True)


def build(args):
    log = (args.case_dir / "stdout").read_text(encoding="utf-8", errors="replace")
    test_checks = [
        {
            "name": name,
            "passed": bool(re.search(rf"test (?:time::)?animation::tests::{re.escape(name)} \.\.\. ok", log)),
        }
        for name in EXPECTED_TESTS
    ]
    fmt_passed = args.format_exit_code == 0
    suite_passed = args.test_exit_code == 0 and all(item["passed"] for item in test_checks)
    status = "REVIEW_READY" if fmt_passed and suite_passed else "CHECK_FAILED"
    now = datetime.now(timezone.utc).isoformat()

    args.case_dir.mkdir(parents=True, exist_ok=True)
    (args.case_dir / "metrics.json").write_text(json.dumps({
        "case_id": CASE_ID,
        "checkpoint": status,
        "format_exit_code": args.format_exit_code,
        "animation_test_exit_code": args.test_exit_code,
        "passed_animation_tests": sum(item["passed"] for item in test_checks),
        "animation_test_count": len(test_checks),
        "palette_joint_capacity": 128,
        "gpu_palette_bytes": 128 * 64 + 16,
        "renderer_consumers_wired": False,
        "scene_window_started": False,
    }, indent=2) + "\n", encoding="utf-8")
    (args.case_dir / "graph.txt").write_text(
        "review checkpoint only: AnimationFrame is not yet connected to GBuffer, Shadow, or WaterReflection\n",
        encoding="utf-8",
    )
    (args.case_dir / "launcher.log").write_text(json.dumps({
        "case_id": CASE_ID,
        "run_id": args.run_id,
        "runner": "t7-skinning-checkpoint/1",
        "runner_pid": args.runner_pid,
        "command": args.command,
        "format_exit_code": args.format_exit_code,
        "animation_test_exit_code": args.test_exit_code,
        "started_at": args.started_at,
        "ended_at": now,
        "os": platform.platform(),
        "machine": platform.machine(),
        "scene_window_started": False,
        "status": status,
    }, indent=2) + "\n", encoding="utf-8")

    rows = "".join(
        f'<tr class="{"pass" if item["passed"] else "fail"}"><td>{esc(item["name"])}</td>'
        f'<td>{"PASS" if item["passed"] else "FAIL"}</td></tr>'
        for item in test_checks
    )
    passed_count = sum(item["passed"] for item in test_checks)
    links = " ".join(
        f'<a href="{name}">{name}</a>'
        for name in ("metrics.json", "launcher.log", "graph.txt", "stdout", "stderr", "fmt.stdout", "fmt.stderr")
    )
    report = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T7.2 palette core — {status}</title>
<style>body{{font:15px/1.5 system-ui,sans-serif;max-width:1050px;margin:2rem auto;padding:0 1rem;color:#172033}}table{{border-collapse:collapse;width:100%;margin:1rem 0}}th,td{{text-align:left;padding:.6rem;border-bottom:1px solid #d0d5dd}}th{{background:#f2f4f7}}.pass td:last-child{{color:#15803d;font-weight:700}}.fail td:last-child{{color:#b42318;font-weight:700}}.status{{padding:.8rem 1rem;border-left:4px solid {'#15803d' if status == 'REVIEW_READY' else '#b42318'};background:{'#f0fdf4' if status == 'REVIEW_READY' else '#fef3f2'}}}code{{overflow-wrap:anywhere}}a{{margin-right:.6rem}}</style></head><body>
<h1>T7.2 — Palette extraction review checkpoint</h1>
<p class="status">Status: <strong>{status}</strong> · {passed_count}/{len(test_checks)} focused tests passed · no scene window started.</p>
<p>This checkpoint covers CPU pose extraction, 128-matrix column-major storage ABI, GPU storage binding/upload, current-pose bounds, local fallback isolation, and stale-generation rejection.</p>
<h2>Focused test results</h2><table><thead><tr><th>Test</th><th>Result</th></tr></thead><tbody>{rows}</tbody></table>
<h2>Remaining before full T7.2 acceptance</h2>
<p>The extracted <code>AnimationFrame</code> is not connected to the GBuffer, Shadow, or WaterReflection render consumers yet. No animated launcher scene or rendered screenshot was captured, so this checkpoint must not be read as full T7.2 acceptance.</p>
<p>The palette builder uses the engine's column-vector convention: <code>global_joint × inverse_bind</code>. This is recorded explicitly because the roadmap currently lists the factors in the opposite order.</p>
<h2>Artifacts</h2><p>{links}</p><p>Run: <code>{esc(args.run_id)}</code> · Commit: <code>{esc(args.commit)}</code> · Format exit: {args.format_exit_code} · Test exit: {args.test_exit_code}</p>
</body></html>"""
    (args.case_dir / "report.html").write_text(report, encoding="utf-8")
    (args.case_dir / "aggregate.json").write_text(json.dumps({
        "case_id": CASE_ID,
        "checkpoint": status,
        "checks": test_checks,
        "format_exit_code": args.format_exit_code,
        "animation_test_exit_code": args.test_exit_code,
        "renderer_consumers_wired": False,
        "scene_window_started": False,
    }, indent=2) + "\n", encoding="utf-8")
    return 0 if status == "REVIEW_READY" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case-dir", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--command", required=True)
    parser.add_argument("--runner-pid", type=int, required=True)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--format-exit-code", type=int, required=True)
    parser.add_argument("--test-exit-code", type=int, required=True)
    args = parser.parse_args()
    try:
        return build(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"could not build T7.2 checkpoint report: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
