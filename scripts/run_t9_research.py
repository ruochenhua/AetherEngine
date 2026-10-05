#!/usr/bin/env python3
"""Run the bounded, offscreen T9.4 GPU and terrain-streaming research case."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "tests" / "fixtures" / "t9_occlusion_streaming.ron"
EXAMPLE = "t9_occlusion_streaming_prototype"
REPORT_BUILDER = ROOT / "scripts" / "build_t9_research_report.py"
REPORT_TEST = ROOT / "scripts" / "test_t9_research_report.py"


def execute(command: list[str], timeout: int) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
        env=os.environ.copy(),
    )


def source_metadata() -> dict[str, object]:
    files = [
        FIXTURE,
        ROOT / "crates" / "aether-engine" / "examples" / f"{EXAMPLE}.rs",
        ROOT / "crates" / "aether-engine" / "examples" / f"{EXAMPLE}.wgsl",
        Path(__file__),
        REPORT_BUILDER,
    ]
    digest = hashlib.sha256()
    for path in files:
        digest.update(path.relative_to(ROOT).as_posix().encode("utf-8"))
        digest.update(path.read_bytes())
    revision = subprocess.run(
        ["git", "rev-parse", "--verify", "HEAD"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    dirty = subprocess.run(
        ["git", "status", "--porcelain"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    return {
        "commit": revision.stdout.strip() if revision.returncode == 0 else "unknown",
        "dirty": bool(dirty.stdout.strip()) if dirty.returncode == 0 else True,
        "source_fingerprint": digest.hexdigest(),
        "started_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
    }


def write_failure(report_dir: Path, title: str, detail: str) -> None:
    (report_dir / "report-status.txt").write_text("FAILED\n", encoding="utf-8")
    (report_dir / "report.html").write_text(
        "<!doctype html><html lang='zh-CN'><meta charset='utf-8'>"
        f"<title>{title}</title><h1>{title}</h1><pre>{detail}</pre>"
        "<p>研究进程已由协调器限制时长；没有启动 launcher 窗口。</p></html>",
        encoding="utf-8",
    )


def main() -> int:
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_dir = ROOT / "tests" / "reports" / f"t9-{run_id}-{os.getpid()}" / "t9_occlusion_streaming_research"
    report_dir.mkdir(parents=True, exist_ok=False)
    (report_dir / "fixture.ron").write_bytes(FIXTURE.read_bytes())
    source = source_metadata()
    stdout: list[str] = []
    stderr: list[str] = []

    print("running focused T9.4 report-contract tests", flush=True)
    try:
        tests = execute([sys.executable, str(REPORT_TEST)], timeout=120)
    except subprocess.TimeoutExpired as error:
        write_failure(report_dir, "T9.4 report-contract test timed out", str(error))
        return 124
    stdout.append(tests.stdout)
    stderr.append(tests.stderr)
    if tests.returncode != 0:
        (report_dir / "stdout").write_text("\n".join(stdout), encoding="utf-8")
        (report_dir / "stderr").write_text("\n".join(stderr), encoding="utf-8")
        write_failure(report_dir, "T9.4 report-contract tests failed", tests.stderr)
        return tests.returncode

    prototype_json = report_dir / "occlusion-prototype.json"
    command = [
        "cargo", "run", "--release", "-p", "aether-engine", "--example", EXAMPLE, "--",
        "--fixture", str(FIXTURE), "--prototype-enabled", "--output", str(prototype_json),
    ]
    (report_dir / "prototype.command.json").write_text(json.dumps(command, indent=2) + "\n", encoding="utf-8")
    print("running offscreen wgpu prototype (30 warm-up + 120 samples)", flush=True)
    try:
        prototype = execute(command, timeout=1200)
    except subprocess.TimeoutExpired as error:
        stdout.append(error.stdout or "")
        stderr.append(error.stderr or "T9.4 prototype timed out\n")
        (report_dir / "stdout").write_text("\n".join(stdout), encoding="utf-8")
        (report_dir / "stderr").write_text("\n".join(stderr), encoding="utf-8")
        write_failure(report_dir, "T9.4 GPU prototype timed out", "The bounded offscreen process exceeded 20 minutes and was terminated.")
        return 124
    stdout.append(prototype.stdout)
    stderr.append(prototype.stderr)
    if prototype.returncode != 0 or not prototype_json.is_file():
        (report_dir / "stdout").write_text("\n".join(stdout), encoding="utf-8")
        (report_dir / "stderr").write_text("\n".join(stderr), encoding="utf-8")
        write_failure(report_dir, "T9.4 GPU prototype failed", prototype.stderr or f"exit code {prototype.returncode}; output missing")
        return prototype.returncode or 1

    (report_dir / "stdout").write_text("\n".join(stdout), encoding="utf-8")
    (report_dir / "stderr").write_text("\n".join(stderr), encoding="utf-8")
    (report_dir / "capture.json").write_text(json.dumps({"source": source, "prototype_exit_code": prototype.returncode, "launcher_started": False}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    report = execute([sys.executable, str(REPORT_BUILDER), "--case-dir", str(report_dir)], timeout=120)
    stdout.append(report.stdout)
    stderr.append(report.stderr)
    (report_dir / "stdout").write_text("\n".join(stdout), encoding="utf-8")
    (report_dir / "stderr").write_text("\n".join(stderr), encoding="utf-8")
    if report.returncode != 0:
        write_failure(report_dir, "T9.4 report validation failed", report.stderr)
        return report.returncode

    print(f"case=t9_occlusion_streaming_research")
    print(f"report={report_dir / 'report.html'}")
    print(f"aggregate={report_dir / 'aggregate.json'}")
    print("research_gate=PASS; production_decision=NO_GO_PRODUCTION")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
