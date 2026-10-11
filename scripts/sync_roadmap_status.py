#!/usr/bin/env python3
"""Render and validate the tracked roadmap status views from roadmap-status.json."""

import argparse
from datetime import date
import html
import json
from pathlib import Path
import posixpath
import subprocess
import sys


ROADMAP = Path("docs/plans/2026-09-17-non-raytracing-engine-roadmap")
MANIFEST = ROADMAP / "roadmap-status.json"
ARCHIVE = ROADMAP / "t0-t6-status-audit.md"
STATUS_ENUMS = {
    "implementation": {"complete", "in_progress", "not_started"},
    "automated": {"passed", "partial", "pending", "not_run"},
    "acceptance": {"accepted", "partial", "pending", "not_applicable"},
    "decision": {None, "research_only", "production_no_go", "production_no_go_research_only"},
}
TASK_IDS = {f"T{index}" for index in range(10)}
OUTPUTS = {
    Path("README.md"): ("<!-- ROADMAP_STATUS:START -->", "<!-- ROADMAP_STATUS:END -->", "readme"),
    Path("README.zh-CN.md"): (
        "<!-- ROADMAP_STATUS_ZH:START -->",
        "<!-- ROADMAP_STATUS_ZH:END -->",
        "readme_zh",
    ),
    ROADMAP / "index.html": ("<!-- ROADMAP_STATUS:START -->", "<!-- ROADMAP_STATUS:END -->", "html"),
    ROADMAP / "verification-current-20261011.md": (
        "<!-- ROADMAP_STATUS:START -->",
        "<!-- ROADMAP_STATUS:END -->",
        "markdown",
    ),
}


class StatusError(ValueError):
    """Invalid roadmap data or generated-document contract."""


def _require(condition, message):
    if not condition:
        raise StatusError(message)


def load_manifest(root):
    path = root / MANIFEST
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise StatusError(f"cannot read roadmap status manifest {path}: {exc}") from exc
    _require(isinstance(data, dict), "manifest must be an object")
    _require(set(data) == {"schema_version", "updated", "tasks"}, "manifest fields must be schema_version, updated, and tasks")
    _require(data.get("schema_version") == 1, "schema_version must be 1")
    try:
        date.fromisoformat(data.get("updated", ""))
    except (TypeError, ValueError) as exc:
        raise StatusError("updated must be an ISO date (YYYY-MM-DD)") from exc
    tasks = data.get("tasks")
    _require(isinstance(tasks, list) and tasks, "tasks must be a non-empty array")
    seen = set()
    task_fields = {"id", "title", "title_zh", *STATUS_ENUMS, "remaining", "remaining_zh", "evidence"}
    for index, task in enumerate(tasks):
        prefix = f"tasks[{index}]"
        _require(isinstance(task, dict), f"{prefix} must be an object")
        _require(set(task) == task_fields, f"{prefix} fields must be {', '.join(sorted(task_fields))}")
        task_id = task.get("id")
        _require(isinstance(task_id, str) and task_id.startswith("T"), f"{prefix}.id must be a task ID")
        _require(task_id not in seen, f"duplicate task ID: {task_id}")
        seen.add(task_id)
        for field, allowed in STATUS_ENUMS.items():
            _require(task.get(field) in allowed, f"{prefix}.{field} has invalid value {task.get(field)!r}")
        _require(isinstance(task.get("title"), str) and task["title"].strip(), f"{prefix}.title is required")
        _require(isinstance(task.get("title_zh"), str) and task["title_zh"].strip(), f"{prefix}.title_zh is required")
        remaining = task.get("remaining")
        _require(isinstance(remaining, list) and all(isinstance(item, str) and item.strip() for item in remaining),
                 f"{prefix}.remaining must be an array of non-empty strings")
        remaining_zh = task.get("remaining_zh")
        _require(isinstance(remaining_zh, list) and all(isinstance(item, str) and item.strip() for item in remaining_zh),
                 f"{prefix}.remaining_zh must be an array of non-empty strings")
        _require(len(remaining) == len(remaining_zh), f"{prefix}.remaining and remaining_zh lengths must match")
        evidence = task.get("evidence")
        _require(isinstance(evidence, list) and evidence, f"{prefix}.evidence must not be empty")
        for evidence_index, item in enumerate(evidence):
            ev_prefix = f"{prefix}.evidence[{evidence_index}]"
            _require(isinstance(item, dict), f"{ev_prefix} must be an object")
            _require(set(item) == {"label", "label_zh", "path"}, f"{ev_prefix} fields must be label, label_zh, and path")
            label, relative = item.get("label"), item.get("path")
            _require(isinstance(label, str) and label.strip(), f"{ev_prefix}.label is required")
            _require(isinstance(item.get("label_zh"), str) and item["label_zh"].strip(), f"{ev_prefix}.label_zh is required")
            _require(isinstance(relative, str) and relative.strip(), f"{ev_prefix}.path is required")
            evidence_path = Path(relative)
            _require(not evidence_path.is_absolute() and ".." not in evidence_path.parts,
                     f"{ev_prefix}.path must stay inside the repository")
            full_path = root / evidence_path
            _require(full_path.is_file(), f"evidence link does not exist: {relative}")
            tracked = subprocess.run(
                ["git", "-C", str(root), "ls-files", "--error-unmatch", "--", relative],
                capture_output=True,
                text=True,
                check=False,
            )
            _require(tracked.returncode == 0, f"evidence link is not tracked by Git: {relative}")
            _require(not relative.startswith(("tests/reports/", "tests/output/")),
                     f"evidence link points into ignored run output: {relative}")
    _require(seen == TASK_IDS, f"task IDs must include exactly T0–T9; missing={sorted(TASK_IDS - seen)}, extra={sorted(seen - TASK_IDS)}")
    return data


def _status_text(task, chinese=False):
    if chinese:
        implementation = {"complete": "实现完成", "in_progress": "进行中", "not_started": "未开始"}[task["implementation"]]
        automated = {"passed": "通过", "partial": "部分通过", "pending": "待验证", "not_run": "未运行"}[task["automated"]]
        acceptance = {"accepted": "已验收", "partial": "部分验收", "pending": "待人工验收", "not_applicable": "不适用"}[task["acceptance"]]
        decision = {
            "research_only": "仅研究",
            "production_no_go": "生产 NO-GO",
            "production_no_go_research_only": "仅研究；生产 NO-GO",
        }.get(task["decision"])
        return "；".join(part for part in (implementation, f"自动验证 {automated}", acceptance, decision) if part)
    implementation = {"complete": "Implemented", "in_progress": "In progress", "not_started": "Not started"}[task["implementation"]]
    automated = {"passed": "passed", "partial": "partial", "pending": "pending", "not_run": "not run"}[task["automated"]]
    acceptance = {"accepted": "accepted", "partial": "partially accepted", "pending": "human review pending", "not_applicable": "not applicable"}[task["acceptance"]]
    decision = {
        "research_only": "research only",
        "production_no_go": "production NO-GO",
        "production_no_go_research_only": "research only; production NO-GO",
    }.get(task["decision"])
    return "; ".join(part for part in (implementation, f"automation {automated}", acceptance, decision) if part)


def _remaining(task, chinese=False):
    items = task["remaining_zh"] if chinese else task["remaining"]
    if not items:
        return "无已知遗留" if chinese else "No known open items"
    return "；".join(items) if chinese else "; ".join(items)


def _evidence_links(task, target, html_mode=False):
    links = []
    for item in task["evidence"]:
        if target in {"readme", "readme_zh"}:
            path = item["path"]
        else:
            path = posixpath.relpath(item["path"], str(ROADMAP))
        label = item["label_zh"] if target == "readme_zh" else item["label"]
        if html_mode:
            links.append(f'<a href="{html.escape(path, quote=True)}">{html.escape(label)}</a>')
        else:
            links.append(f'[{label}]({path})')
    return " · ".join(links)


def render(kind, data):
    date_text = data["updated"]
    tasks = data["tasks"]
    if kind in {"readme", "readme_zh", "markdown"}:
        chinese = kind == "readme_zh"
        if chinese:
            lines = [f"**状态同步日期：{date_text}。** 人工验收与自动验证分开记录；截图采集本身不会自动通过视觉验收。", "", "| 任务 | 实现 / 自动验证 / 验收 | 剩余项 | 证据 |", "| --- | --- | --- | --- |"]
        else:
            lines = [f"**Status synced {date_text}.** Automated checks and human acceptance are tracked separately; a capture alone does not approve visual results.", "", "| Task | Implementation / automation / acceptance | Remaining | Evidence |", "| --- | --- | --- | --- |"]
        for task in tasks:
            evidence = _evidence_links(task, "readme_zh" if chinese else ("readme" if kind == "readme" else "markdown"))
            title = task["title_zh"] if chinese else task["title"]
            lines.append(f"| {task['id']} · {title} | {_status_text(task, chinese)} | {_remaining(task, chinese)} | {evidence} |")
        return "\n".join(lines)
    if kind == "html":
        rows = []
        for task in tasks:
            links = _evidence_links(task, "html", html_mode=True)
            rows.append(
                "<tr><td><strong>{}</strong><br>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>".format(
                    html.escape(task["id"]), html.escape(task["title"]), html.escape(_status_text(task)),
                    html.escape(_remaining(task)), links,
                )
            )
        return (
            f'<section class="callout blue"><strong>Machine-synced status · {html.escape(date_text)}</strong>'
            '<p>Automation and human acceptance are tracked separately. A capture alone does not approve visual results.</p>'
            '<table><thead><tr><th>Task</th><th>Implementation / automation / acceptance</th><th>Remaining</th><th>Evidence</th></tr></thead>'
            f'<tbody>{"".join(rows)}</tbody></table></section>'
        )
    raise StatusError(f"unknown output kind: {kind}")


def replace_block(content, start, end, body):
    _require(content.count(start) == 1 and content.count(end) == 1, f"expected one generated block: {start}")
    start_index = content.index(start) + len(start)
    end_index = content.index(end)
    _require(start_index <= end_index, f"generated block markers are reversed: {start}")
    return content[:start_index] + "\n" + body + "\n" + content[end_index:]


def check_archive(root):
    path = root / ARCHIVE
    _require(path.is_file(), f"historical audit file is missing: {ARCHIVE}")
    content = path.read_text(encoding="utf-8")
    _require("历史档案" in content and "verification-current-20261011.md" in content,
             f"old T0-T6 audit must be marked as an archive and link to current status: {ARCHIVE}")


def sync(root, mode):
    data = load_manifest(root)
    check_archive(root)
    stale = []
    for relative, (start, end, kind) in OUTPUTS.items():
        path = root / relative
        _require(path.is_file(), f"generated status document is missing: {relative}")
        before = path.read_text(encoding="utf-8")
        after = replace_block(before, start, end, render(kind, data))
        if before != after:
            if mode == "--write":
                path.write_text(after, encoding="utf-8")
            else:
                stale.append(str(relative))
    if stale:
        raise StatusError("generated status blocks are stale: " + ", ".join(stale) + "; run --write")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="regenerate tracked status blocks")
    mode.add_argument("--check", action="store_true", help="validate manifest, evidence, and generated blocks")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1], help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        sync(args.root.resolve(), "--write" if args.write else "--check")
    except StatusError as exc:
        print(f"roadmap status error: {exc}", file=sys.stderr)
        return 1
    print("Roadmap status synchronized." if args.write else "Roadmap status is current.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
