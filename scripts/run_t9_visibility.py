#!/usr/bin/env python3
"""Run the bounded T9.2 1k/10k visibility and LOD captures."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
REPORT_SCRIPT = ROOT / "scripts" / "build_t9_visibility_report.py"
CASE_FIXTURE = ROOT / "tests" / "fixtures" / "t9_visibility_case.json"
SCENE_BUILDER = ROOT / "scripts" / "build_t9_baseline_scene.py"
FINGERPRINTER = ROOT / "scripts" / "fingerprint_working_tree.py"
VARIANTS = ((1000, "entities_1k"), (10000, "entities_10k"))


def run(
    command: list[str],
    timeout: int,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def failure_report(report_dir: Path, title: str, detail: str) -> None:
    page = (
        "<!doctype html><html lang='en'><meta charset='utf-8'><title>T9.2 capture failed</title>"
        "<body style='font:16px/1.5 system-ui;max-width:900px;margin:2rem auto'>"
        f"<h1>{title}</h1><pre>{detail}</pre>"
        "<p>Each launcher capture is bounded to 150 frames and is stopped by its supervisor on timeout.</p>"
        "</body></html>"
    )
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    (report_dir / "report-status.txt").write_text("FAILED\n", encoding="utf-8")


def capture_case(
    report_dir: Path,
    launcher: Path,
    source: dict[str, str | bool],
    entity_count: int,
    scene_stem: str,
    env: dict[str, str],
) -> bool:
    scene_path = report_dir / f"{scene_stem}.ron"
    case_path = report_dir / f"{scene_stem}.case.json"
    benchmark_path = report_dir / f"{scene_stem}.benchmark.json"
    visibility_path = report_dir / f"{scene_stem}.visibility.json"
    screenshot_path = report_dir / f"{scene_stem}.png"
    builder = run(
        [
            sys.executable,
            str(SCENE_BUILDER),
            "--output",
            str(scene_path),
            "--seed",
            "9001",
            "--entity-count",
            str(entity_count),
            "--extent",
            "55" if entity_count == 1000 else "175",
            "--lod-zones",
            "--scene-name",
            f"T9.2 Visibility LOD {entity_count}",
        ],
        timeout=30,
    )
    (report_dir / f"{scene_stem}.scene-builder.stdout").write_text(
        builder.stdout, encoding="utf-8"
    )
    (report_dir / f"{scene_stem}.scene-builder.stderr").write_text(
        builder.stderr, encoding="utf-8"
    )
    if builder.returncode != 0:
        return False

    case = json.loads(CASE_FIXTURE.read_text(encoding="utf-8"))
    case["case_id"] = f"t9_visibility_lod_{entity_count}"
    case["scene"] = scene_stem
    case["entity_count"] = entity_count
    case["fixture_source"] = (
        f"scripts/build_t9_baseline_scene.py sha256="
        f"{hashlib.sha256(SCENE_BUILDER.read_bytes()).hexdigest()}; "
        f"generated-scene-sha256={hashlib.sha256(scene_path.read_bytes()).hexdigest()}"
    )
    case_path.write_text(json.dumps(case, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    command = [
        str(launcher),
        "--scene",
        str(scene_path),
        "--screenshot",
        str(screenshot_path),
        "--exit-after-frames",
        "150",
        "--no-gui-overlay",
        "--no-debug-helpers",
        "--width",
        "1280",
        "--height",
        "720",
        "--time-mode",
        "fixed-step",
        "--simulation-time",
        "0",
        "--fixed-dt",
        "0.016666667",
        "--benchmark-case",
        str(case_path),
        "--benchmark-output",
        str(benchmark_path),
        "--benchmark-visibility-output",
        str(visibility_path),
        "--benchmark-commit",
        str(source["commit"]),
        "--benchmark-source-fingerprint",
        str(source["fingerprint"]),
        "--benchmark-source-dirty",
        str(source["dirty"]).lower(),
    ]
    (report_dir / f"{scene_stem}.command.json").write_text(
        json.dumps(command, indent=2) + "\n", encoding="utf-8"
    )
    try:
        completed = run(command, timeout=300, env=env)
    except subprocess.TimeoutExpired as error:
        (report_dir / f"{scene_stem}.launcher.stdout").write_text(
            error.stdout or "", encoding="utf-8"
        )
        (report_dir / f"{scene_stem}.launcher.stderr").write_text(
            error.stderr or "bounded capture timed out\n", encoding="utf-8"
        )
        return False
    (report_dir / f"{scene_stem}.launcher.stdout").write_text(
        completed.stdout, encoding="utf-8"
    )
    (report_dir / f"{scene_stem}.launcher.stderr").write_text(
        completed.stderr, encoding="utf-8"
    )
    return (
        completed.returncode == 0
        and benchmark_path.is_file()
        and visibility_path.is_file()
        and screenshot_path.is_file()
    )


def main() -> int:
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_dir = ROOT / "tests" / "reports" / f"t9-{run_id}" / "t9_visibility_lod_1k_10k"
    report_dir.mkdir(parents=True, exist_ok=False)

    focused_commands = [
        [
            "cargo",
            "test",
            "-p",
            "aether-engine",
            "--lib",
            "renderer::visibility::tests",
        ],
        ["cargo", "test", "-p", "aether-launcher", "visibility_"],
        [sys.executable, str(ROOT / "scripts" / "test_t9_visibility_report.py")],
    ]
    for index, command in enumerate(focused_commands, start=1):
        print(f"running focused T9.2 verification {index}/{len(focused_commands)}", flush=True)
        try:
            result = run(command, timeout=1200)
        except subprocess.TimeoutExpired as error:
            (report_dir / f"focused-test-{index}.stdout").write_text(
                error.stdout or "", encoding="utf-8"
            )
            (report_dir / f"focused-test-{index}.stderr").write_text(
                error.stderr or "test timed out\n", encoding="utf-8"
            )
            failure_report(report_dir, "T9.2 focused tests timed out", f"Test {index} exceeded 20 minutes.")
            return 124
        (report_dir / f"focused-test-{index}.stdout").write_text(
            result.stdout, encoding="utf-8"
        )
        (report_dir / f"focused-test-{index}.stderr").write_text(
            result.stderr, encoding="utf-8"
        )
        if result.returncode != 0:
            failure_report(report_dir, "T9.2 focused tests failed", f"See focused-test-{index} logs.")
            return result.returncode
    unit_log = (report_dir / "focused-test-1.stdout").read_text(encoding="utf-8")
    (report_dir / "visibility-unit-tests.stdout").write_text(unit_log, encoding="utf-8")

    fingerprint = run([sys.executable, str(FINGERPRINTER)], timeout=30)
    if fingerprint.returncode != 0:
        failure_report(report_dir, "T9.2 source fingerprint failed", fingerprint.stderr)
        return fingerprint.returncode
    source = json.loads(fingerprint.stdout)

    build_dir = ROOT / "target" / "t9-benchmark"
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(build_dir)
    print("building the isolated release launcher", flush=True)
    try:
        build = run(["cargo", "build", "-p", "aether-launcher", "--release"], 1200, env)
    except subprocess.TimeoutExpired as error:
        (report_dir / "build.stdout").write_text(error.stdout or "", encoding="utf-8")
        (report_dir / "build.stderr").write_text(
            error.stderr or "release build timed out\n", encoding="utf-8"
        )
        failure_report(report_dir, "T9.2 release build timed out", "The isolated build exceeded 20 minutes.")
        return 124
    (report_dir / "build.stdout").write_text(build.stdout, encoding="utf-8")
    (report_dir / "build.stderr").write_text(build.stderr, encoding="utf-8")
    if build.returncode != 0:
        failure_report(report_dir, "T9.2 release build failed", "See build.stdout and build.stderr.")
        return build.returncode

    launcher_name = "aether-launcher.exe" if os.name == "nt" else "aether-launcher"
    launcher = build_dir / "release" / launcher_name
    for entity_count, scene_stem in VARIANTS:
        print(f"capturing {entity_count:,}-entity scene with auto-close", flush=True)
        if not capture_case(report_dir, launcher, source, entity_count, scene_stem, env):
            failure_report(
                report_dir,
                f"T9.2 {entity_count:,}-entity capture failed",
                f"Inspect {scene_stem}.launcher.stderr and the per-case artifacts.",
            )
            return 1

    report_result = run(
        [sys.executable, str(REPORT_SCRIPT), "--report-dir", str(report_dir)],
        timeout=30,
    )
    print(f"report={report_dir / 'report.html'}")
    if report_result.returncode != 0:
        return report_result.returncode
    print(f"report_status={(report_dir / 'report-status.txt').read_text().strip()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
