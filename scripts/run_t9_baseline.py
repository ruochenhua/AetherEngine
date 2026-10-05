#!/usr/bin/env python3
"""Build and run the bounded 30+120-frame T9.1 capture."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
REPORT_SCRIPT = ROOT / "scripts" / "build_t9_benchmark_report.py"
CASE_FIXTURE = ROOT / "tests" / "fixtures" / "t9_benchmark_case.json"
SCENE_BUILDER = ROOT / "scripts" / "build_t9_baseline_scene.py"
FINGERPRINTER = ROOT / "scripts" / "fingerprint_working_tree.py"


def run(command: list[str], timeout: int, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
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
        "<!doctype html><html lang='en'><meta charset='utf-8'><title>T9.1 capture failed</title>"
        "<body style='font:16px/1.5 system-ui;max-width:900px;margin:2rem auto'>"
        f"<h1>{title}</h1><p>{detail}</p>"
        "<p>The bounded launcher was terminated by its supervisor if its timeout expired.</p>"
        "</body></html>"
    )
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    (report_dir / "report-status.txt").write_text("FAILED\n", encoding="utf-8")


def main() -> int:
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_dir = ROOT / "tests" / "reports" / f"t9-{run_id}" / "t9_baseline_report"
    report_dir.mkdir(parents=True, exist_ok=False)
    scene_path = report_dir / "t9_baseline_report.ron"
    case_path = report_dir / "benchmark-case.json"
    report_path = report_dir / "benchmark.json"
    screenshot_path = report_dir / "output.png"

    scene_result = run(
        [sys.executable, str(SCENE_BUILDER), "--output", str(scene_path), "--seed", "9001"],
        timeout=30,
    )
    if scene_result.returncode != 0:
        (report_dir / "scene-builder.stderr").write_text(scene_result.stderr, encoding="utf-8")
        failure_report(report_dir, "T9.1 fixture generation failed", scene_result.stderr)
        return scene_result.returncode

    case = json.loads(CASE_FIXTURE.read_text(encoding="utf-8"))
    scene_hash = hashlib.sha256(scene_path.read_bytes()).hexdigest()
    source_hash = hashlib.sha256(SCENE_BUILDER.read_bytes()).hexdigest()
    case["fixture_source"] = (
        f"scripts/build_t9_baseline_scene.py sha256={source_hash}; "
        f"generated-scene-sha256={scene_hash}"
    )
    case_path.write_text(json.dumps(case, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    fingerprint_result = run([sys.executable, str(FINGERPRINTER)], timeout=30)
    if fingerprint_result.returncode != 0:
        (report_dir / "fingerprint.stderr").write_text(fingerprint_result.stderr, encoding="utf-8")
        failure_report(report_dir, "T9.1 source fingerprint failed", fingerprint_result.stderr)
        return fingerprint_result.returncode
    source = json.loads(fingerprint_result.stdout)

    test_commands = [
        ["cargo", "test", "-p", "aether-engine", "--test", "performance_series"],
        ["cargo", "test", "-p", "aether-launcher", "benchmark_capture_"],
        [sys.executable, str(ROOT / "scripts" / "test_t9_benchmark_report.py")],
    ]
    for index, test_command in enumerate(test_commands, start=1):
        print(f"running focused T9.1 verification {index}/3", flush=True)
        try:
            test = run(test_command, timeout=1200)
        except subprocess.TimeoutExpired as error:
            (report_dir / f"focused-test-{index}.stdout").write_text(error.stdout or "", encoding="utf-8")
            (report_dir / f"focused-test-{index}.stderr").write_text(error.stderr or "test timed out\n", encoding="utf-8")
            failure_report(report_dir, "T9.1 focused tests timed out", f"Test command {index} exceeded 20 minutes.")
            return 124
        (report_dir / f"focused-test-{index}.stdout").write_text(test.stdout, encoding="utf-8")
        (report_dir / f"focused-test-{index}.stderr").write_text(test.stderr, encoding="utf-8")
        if test.returncode != 0:
            failure_report(report_dir, "T9.1 focused tests failed", f"See focused-test-{index} logs.")
            return test.returncode

    build_dir = ROOT / "target" / "t9-benchmark"
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(build_dir)
    build_command = ["cargo", "build", "-p", "aether-launcher", "--release"]
    print("building the isolated release launcher", flush=True)
    try:
        build = run(build_command, timeout=1200, env=env)
    except subprocess.TimeoutExpired as error:
        (report_dir / "build.stdout").write_text(error.stdout or "", encoding="utf-8")
        (report_dir / "build.stderr").write_text(error.stderr or "release build timed out\n", encoding="utf-8")
        failure_report(report_dir, "T9.1 release build timed out", "The isolated benchmark build exceeded 20 minutes.")
        return 124
    (report_dir / "build.stdout").write_text(build.stdout, encoding="utf-8")
    (report_dir / "build.stderr").write_text(build.stderr, encoding="utf-8")
    if build.returncode != 0:
        failure_report(report_dir, "T9.1 release build failed", "See build.stdout and build.stderr.")
        return build.returncode

    binary_name = "aether-launcher.exe" if os.name == "nt" else "aether-launcher"
    launcher = build_dir / "release" / binary_name
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
        str(report_path),
        "--benchmark-commit",
        source["commit"],
        "--benchmark-source-fingerprint",
        source["fingerprint"],
        "--benchmark-source-dirty",
        str(source["dirty"]).lower(),
    ]
    (report_dir / "command.json").write_text(json.dumps(command, indent=2) + "\n", encoding="utf-8")
    print("running one hidden, auto-closing 150-frame capture", flush=True)
    try:
        launcher_run = run(command, timeout=240, env=env)
    except subprocess.TimeoutExpired as error:
        (report_dir / "launcher.stdout").write_text(error.stdout or "", encoding="utf-8")
        (report_dir / "launcher.stderr").write_text(error.stderr or "bounded capture timed out\n", encoding="utf-8")
        failure_report(report_dir, "T9.1 bounded capture timed out", "The launcher exceeded 240 seconds and was stopped.")
        return 124
    (report_dir / "launcher.stdout").write_text(launcher_run.stdout, encoding="utf-8")
    (report_dir / "launcher.stderr").write_text(launcher_run.stderr, encoding="utf-8")
    if launcher_run.returncode != 0:
        failure_report(report_dir, "T9.1 launcher capture failed", "See launcher.stdout and launcher.stderr.")
        return launcher_run.returncode
    if not report_path.is_file() or not screenshot_path.is_file():
        failure_report(report_dir, "T9.1 artifacts are incomplete", "The bounded launcher exited without JSON or PNG output.")
        return 1

    report_result = run(
        [sys.executable, str(REPORT_SCRIPT), "--report-dir", str(report_dir)],
        timeout=30,
    )
    if report_result.returncode != 0:
        failure_report(report_dir, "T9.1 series is incomplete", "The HTML summary marks this capture incomplete.")
        return report_result.returncode
    series = json.loads(report_path.read_text(encoding="utf-8"))
    valid = (
        series["status"] == "Complete"
        and len(series["samples"]) == 120
        and all(sample["visible_entities"] == 1000 for sample in series["samples"])
        and series["rgba8_sha256"] is not None
    )
    print(f"report={report_dir / 'report.html'}")
    print(f"benchmark={report_path}")
    print(f"series_status={series['status']}")
    print(f"baseline_revision_match={series['commit'] == case['baseline_commit'] and not series['source_tree_dirty']}")
    return 0 if valid else 1


if __name__ == "__main__":
    raise SystemExit(main())
