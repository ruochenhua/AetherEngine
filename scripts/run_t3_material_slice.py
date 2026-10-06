"""Run T3.2 checks and capture active/legacy material paths for review."""

from datetime import datetime, timezone
import html
import json
from pathlib import Path
import subprocess
import sys

from run_visual_regression import ROOT, launcher_binary, run
from visual_report import write_report


CASES = ["t3_pbr_material_grid", "t3_pbr_material_grid_legacy"]
COMMANDS = [
    ["cargo", "test", "--locked", "-p", "aether-engine", "--lib"],
    ["cargo", "test", "--locked", "-p", "aether-launcher"],
    ["cargo", "build", "--locked", "-p", "aether-launcher", "--release"],
]


def command_name(command: list[str]) -> str:
    return "_".join(command[1:]).replace("--", "").replace("-", "_")


def write_review(directory: Path, run_id: str, checks: list[dict], records: list[dict]) -> Path:
    cases = []
    for record in records:
        case_dir = Path("captures") / record["id"]
        launcher = record["launcher"]
        cases.append(
            "<section><h2>"
            + html.escape(record["id"])
            + "</h2><p>状态：<strong>"
            + html.escape(record["status"])
            + "</strong>；launcher exit code："
            + html.escape(str(launcher["exit_code"]))
            + "；进程组已清空："
            + html.escape(str(launcher["group_empty"]))
            + "</p><p><a href=\""
            + str(case_dir / "report.html").replace("\\", "/")
            + "\">打开完整运行报告</a></p><img style=\"max-width:100%;height:auto\" src=\""
            + str(case_dir / "output.png").replace("\\", "/")
            + "\" alt=\""
            + html.escape(record["id"])
            + ' screenshot"></section>'
        )
    check_rows = "".join(
        "<tr><td>"
        + html.escape(item["name"])
        + "</td><td>"
        + html.escape(" ".join(item["command"]))
        + "</td><td>"
        + html.escape(str(item["exit_code"]))
        + "</td></tr>"
        for item in checks
    )
    content = (
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>T3.2 材质路径人工验收</title>"
        "<style>body{font:15px/1.6 system-ui,sans-serif;max-width:1100px;margin:32px auto;padding:0 18px;color:#182230}"
        "table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}"
        "section{margin:28px 0;padding:16px;border:1px solid #d0d5dd;border-radius:10px}"
        "</style><body><h1>T3.2 材质提取对照</h1><p>本报告并排展示 ExtractedMaterial 活动路径和旧 MaterialUniform 兼容路径。"
        "截图只用于人工验收；未自动提升 reference。每次 launcher 采集均要求进程组清空。</p>"
        "<h2>自动检查</h2><table><tr><th>检查</th><th>命令</th><th>退出码</th></tr>"
        + check_rows
        + "</table><h2>Release 场景截图</h2>"
        + "".join(cases)
        + "</body></html>"
    )
    target = directory / "final-review.html"
    target.write_text(content, encoding="utf-8")
    return target


def main() -> int:
    import os

    run_id = "t3.2-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + f"-{os.getpid()}"
    directory = ROOT / "tests" / "reports" / run_id
    directory.mkdir(parents=True)
    checks = []
    for command in COMMANDS:
        name = command_name(command)
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
        (directory / f"{name}.stdout").write_text(result.stdout, encoding="utf-8")
        (directory / f"{name}.stderr").write_text(result.stderr, encoding="utf-8")
        checks.append({"name": name, "command": command, "exit_code": result.returncode})
        if result.returncode:
            (directory / "checks.json").write_text(json.dumps(checks, indent=2), encoding="utf-8")
            write_review(directory, run_id, checks, [])
            print(f"T3.2 check failed; report={directory / 'final-review.html'}", file=sys.stderr)
            return result.returncode

    launcher = launcher_binary(ROOT)
    records, cancelled = run(
        ROOT / "tests" / "visual-matrix.json",
        CASES,
        directory / "captures",
        launcher,
        root=ROOT,
        capture_reference=True,
        settle=0.25,
    )
    source_commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip())
    for record in records:
        record["source_worktree"] = {"commit": source_commit, "dirty": dirty}
        candidate_path = directory / "captures" / record["id"] / "candidate.json"
        candidate_path.write_text(json.dumps(record, indent=2), encoding="utf-8")
        write_report(directory / "captures" / record["id"], record)
    (directory / "checks.json").write_text(json.dumps(checks, indent=2), encoding="utf-8")
    (directory / "captures.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    review = write_review(directory, run_id, checks, records)
    print(f"T3.2 report: {review}")
    if cancelled or len(records) != len(CASES):
        return 1
    if any(
        record["status"] != "reference-pending"
        or record["launcher"]["exit_code"] != 0
        or not record["launcher"]["group_empty"]
        for record in records
    ):
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
