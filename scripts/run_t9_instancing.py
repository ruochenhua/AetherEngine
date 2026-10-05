#!/usr/bin/env python3
"""Run the bounded T9.3 paired 10k instancing capture."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CASE_FIXTURE = ROOT / "tests" / "fixtures" / "t9_instancing_case.json"
SCENE_BUILDER = ROOT / "scripts" / "build_t9_baseline_scene.py"
FINGERPRINTER = ROOT / "scripts" / "fingerprint_working_tree.py"
REPORT_SCRIPT = ROOT / "scripts" / "build_t9_instancing_report.py"


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
        "<!doctype html><html lang='en'><meta charset='utf-8'>"
        "<title>T9.3 capture failed</title>"
        "<body style='font:16px/1.5 system-ui;max-width:900px;margin:2rem auto'>"
        f"<h1>{title}</h1><pre>{detail}</pre>"
        "<p>Each launcher capture is bounded to 150 frames and its process exits automatically.</p>"
        "</body></html>"
    )
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    (report_dir / "report-status.txt").write_text("FAILED\n", encoding="utf-8")


def run_logged(
    command: list[str],
    timeout: int,
    report_dir: Path,
    stem: str,
    env: dict[str, str] | None = None,
) -> int:
    try:
        completed = run(command, timeout=timeout, env=env)
    except subprocess.TimeoutExpired as error:
        (report_dir / f"{stem}.stdout").write_text(error.stdout or "", encoding="utf-8")
        (report_dir / f"{stem}.stderr").write_text(
            error.stderr or f"{stem} timed out\n", encoding="utf-8"
        )
        return 124
    (report_dir / f"{stem}.stdout").write_text(completed.stdout, encoding="utf-8")
    (report_dir / f"{stem}.stderr").write_text(completed.stderr, encoding="utf-8")
    return completed.returncode


def prepare_fixture(report_dir: Path) -> tuple[Path, str] | None:
    scene_path = report_dir / "t9_instancing_paired_10k.ron"
    command = [
        sys.executable,
        str(SCENE_BUILDER),
        "--output",
        str(scene_path),
        "--seed",
        "9003",
        "--entity-count",
        "10000",
        "--extent",
        "175",
        "--scene-name",
        "T9.3 Paired Instancing 10000",
    ]
    builder = run(command, timeout=30)
    (report_dir / "scene-builder.stdout").write_text(builder.stdout, encoding="utf-8")
    (report_dir / "scene-builder.stderr").write_text(builder.stderr, encoding="utf-8")
    if builder.returncode != 0:
        return None
    source = (
        f"scripts/build_t9_baseline_scene.py sha256="
        f"{hashlib.sha256(SCENE_BUILDER.read_bytes()).hexdigest()}; "
        f"generated-scene-sha256={hashlib.sha256(scene_path.read_bytes()).hexdigest()}"
    )
    return scene_path, source


def write_case(report_dir: Path, variant: str, fixture_source: str, enabled: bool) -> Path:
    case = json.loads(CASE_FIXTURE.read_text(encoding="utf-8"))
    case["fixture_source"] = fixture_source
    case["feature_flags"]["instancing"] = enabled
    path = report_dir / f"{variant}.case.json"
    path.write_text(json.dumps(case, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def capture(
    report_dir: Path,
    launcher: Path,
    scene: Path,
    source: dict[str, str | bool],
    variant: str,
    fixture_source: str,
    enabled: bool,
    env: dict[str, str],
) -> int:
    case_path = write_case(report_dir, variant, fixture_source, enabled)
    benchmark_path = report_dir / f"{variant}.benchmark.json"
    visibility_path = report_dir / f"{variant}.visibility.json"
    screenshot_path = report_dir / f"{variant}.png"
    command = [
        str(launcher),
        "--scene",
        str(scene),
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
    (report_dir / f"{variant}.command.json").write_text(
        json.dumps(command, indent=2) + "\n", encoding="utf-8"
    )
    try:
        completed = run(command, timeout=600, env=env)
    except subprocess.TimeoutExpired as error:
        (report_dir / f"{variant}.launcher.stdout").write_text(
            error.stdout or "", encoding="utf-8"
        )
        (report_dir / f"{variant}.launcher.stderr").write_text(
            error.stderr or "bounded capture timed out\n", encoding="utf-8"
        )
        return 124
    (report_dir / f"{variant}.launcher.stdout").write_text(
        completed.stdout, encoding="utf-8"
    )
    (report_dir / f"{variant}.launcher.stderr").write_text(
        completed.stderr, encoding="utf-8"
    )
    expected = (benchmark_path, visibility_path, screenshot_path)
    if completed.returncode != 0 or not all(path.is_file() for path in expected):
        return completed.returncode or 1
    return 0


def main() -> int:
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_dir = ROOT / "tests" / "reports" / f"t9-{run_id}" / "t9_instancing_paired_10k"
    report_dir.mkdir(parents=True, exist_ok=False)

    focused_commands = [
        [
            "cargo", "test", "-p", "aether-engine", "--lib",
            "renderer::extract::tests::opaque_instancing_toggle_preserves_entities_and_stable_order",
        ],
        [
            "cargo", "test", "-p", "aether-engine", "--lib",
            "state_sha256_is_stable_and_changes_with_world_state",
        ],
        [sys.executable, str(ROOT / "scripts" / "test_t9_instancing_report.py")],
    ]
    for index, command in enumerate(focused_commands, start=1):
        print(f"running focused T9.3 verification {index}/{len(focused_commands)}", flush=True)
        try:
            result = run(command, timeout=1200)
        except subprocess.TimeoutExpired as error:
            (report_dir / f"focused-test-{index}.stdout").write_text(
                error.stdout or "", encoding="utf-8"
            )
            (report_dir / f"focused-test-{index}.stderr").write_text(
                error.stderr or "focused test timed out\n", encoding="utf-8"
            )
            failure_report(report_dir, "T9.3 focused tests timed out", f"Test {index} exceeded 20 minutes.")
            return 124
        (report_dir / f"focused-test-{index}.stdout").write_text(
            result.stdout, encoding="utf-8"
        )
        (report_dir / f"focused-test-{index}.stderr").write_text(
            result.stderr, encoding="utf-8"
        )
        if result.returncode != 0:
            failure_report(report_dir, "T9.3 focused tests failed", f"See focused-test-{index} logs.")
            return result.returncode

    fingerprint = run([sys.executable, str(FINGERPRINTER)], timeout=30)
    if fingerprint.returncode != 0:
        failure_report(report_dir, "T9.3 source fingerprint failed", fingerprint.stderr)
        return fingerprint.returncode
    source = json.loads(fingerprint.stdout)

    build_dir = ROOT / "target" / "t9-benchmark"
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(build_dir)
    print("building the isolated release launcher", flush=True)
    build_exit = run_logged(
        ["cargo", "build", "-p", "aether-launcher", "--release"],
        timeout=1200,
        report_dir=report_dir,
        stem="release-build",
        env=env,
    )
    if build_exit != 0:
        failure_report(report_dir, "T9.3 release build failed", "See release-build.stdout and release-build.stderr.")
        return build_exit

    prepared = prepare_fixture(report_dir)
    if prepared is None:
        failure_report(report_dir, "T9.3 fixture generation failed", "See scene-builder logs.")
        return 1
    scene, fixture_source = prepared
    launcher_name = "aether-launcher.exe" if os.name == "nt" else "aether-launcher"
    launcher = build_dir / "release" / launcher_name
    for variant, enabled in (("baseline", False), ("feature", True)):
        mode = "on" if enabled else "off"
        print(f"capturing 10,000 entities with instancing {mode}; launcher auto-closes", flush=True)
        exit_code = capture(
            report_dir, launcher, scene, source, variant, fixture_source, enabled, env
        )
        if exit_code != 0:
            failure_report(
                report_dir,
                f"T9.3 {variant} capture failed",
                f"Inspect {variant}.launcher.stderr and the per-case artifacts.",
            )
            return exit_code

    report_exit = run_logged(
        [sys.executable, str(REPORT_SCRIPT), "--report-dir", str(report_dir)],
        timeout=30,
        report_dir=report_dir,
        stem="report-builder",
    )
    print(f"report={report_dir / 'report.html'}")
    if report_exit != 0:
        print(f"report_status={(report_dir / 'report-status.txt').read_text().strip()}")
    return report_exit


if __name__ == "__main__":
    raise SystemExit(main())
