"""Run T4.1's GPU composite sentinels and capture a real scheduler graph."""

from datetime import datetime, timezone
import html
import json
from pathlib import Path
import subprocess
import sys

from run_visual_regression import ROOT, launcher_binary, run
from visual_report import write_report


CASE = "t4_transparent_resource_graph"
COMMANDS = [
    [
        "cargo",
        "test",
        "--locked",
        "-p",
        "aether-engine",
        "--lib",
        "production_composite_shader_reads_back_t4_gpu_sentinels",
    ],
    [
        "cargo",
        "test",
        "--locked",
        "-p",
        "aether-engine",
        "--lib",
        "graph_artifact_contains_schedule_and_typed_resource_edges",
    ],
    [
        "cargo",
        "test",
        "--locked",
        "-p",
        "aether-engine",
        "--lib",
        "graph_artifact_tracks_transparent_binding_and_composite_overlay_order",
    ],
    [
        "cargo",
        "test",
        "--locked",
        "-p",
        "aether-launcher",
        "render_graph_output_accepts_one_artifact_path",
    ],
    ["cargo", "build", "--locked", "-p", "aether-launcher", "--release"],
]
EXPECTED_PHASE_ORDER = [
    "SSR",
    "VolumetricCloud",
    "GodRay",
    "WaterReflection",
    "Transparent",
    "Water",
    "Composite",
]
EXPECTED_COMPOSITE_ORDER = [
    "TransparentColor",
    "CloudColor",
    "GodRayColor",
    "WaterColor",
]


def run_checks(directory: Path) -> list[dict]:
    results = []
    for command in COMMANDS:
        result = subprocess.run(
            command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8"
        )
        name = "_".join(command[1:]).replace("--", "").replace("-", "_")
        (directory / f"{name}.stdout").write_text(result.stdout, encoding="utf-8")
        (directory / f"{name}.stderr").write_text(result.stderr, encoding="utf-8")
        results.append({"command": command, "exit_code": result.returncode})
        if result.returncode:
            break
    (directory / "checks.json").write_text(
        json.dumps(results, indent=2), encoding="utf-8"
    )
    return results


def validate_graph(case_directory: Path) -> dict:
    graph_path = case_directory / "render-graph.json"
    graph = json.loads(graph_path.read_text(encoding="utf-8"))
    order = graph["execution_order"]
    positions = {name: order.index(name) for name in EXPECTED_PHASE_ORDER}
    phase_order_valid = all(
        positions[left] < positions[right]
        for left, right in zip(EXPECTED_PHASE_ORDER, EXPECTED_PHASE_ORDER[1:])
    )
    edge_valid = any(
        edge["from"] == "Transparent"
        and edge["to"] == "Composite"
        and edge["resource"] == "transparent_color"
        and edge["kind"] == "resource-read"
        for edge in graph["resource_edges"]
    )
    overlay_order_valid = graph["composite_overlay_order"] == EXPECTED_COMPOSITE_ORDER
    result = {
        "schema_version": graph["schema_version"],
        "pass_order": order,
        "phase_order_valid": phase_order_valid,
        "transparent_to_composite_edge_valid": edge_valid,
        "composite_overlay_order": graph["composite_overlay_order"],
        "composite_overlay_order_valid": overlay_order_valid,
    }
    (case_directory / "graph-expectations.json").write_text(
        json.dumps(result, indent=2), encoding="utf-8"
    )
    if not all(
        [phase_order_valid, edge_valid, overlay_order_valid]
    ):
        raise ValueError("render graph does not satisfy the T4.1 expectations")
    return result


def write_review(directory: Path, checks: list[dict], record: dict, graph: dict) -> Path:
    case = record["id"]
    case_directory = directory / case
    rows = "".join(
        "<tr><td>"
        + html.escape(" ".join(item["command"]))
        + "</td><td>"
        + str(item["exit_code"])
        + "</td></tr>"
        for item in checks
    )
    links = " · ".join(
        f'<a href="captures/{html.escape(case)}/{name}">{name}</a>'
        for name in ["render-graph.json", "graph-expectations.json", "aggregate.json"]
    )
    content = (
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>T4.1 GPU Graph 验收</title>"
        "<style>body{font:15px/1.6 system-ui,sans-serif;max-width:1100px;margin:32px auto;padding:0 18px;color:#182230}"
        "table{border-collapse:collapse;width:100%}td,th{border:1px solid #d0d5dd;padding:8px;text-align:left}"
        "img{max-width:100%;height:auto;border:1px solid #d0d5dd}pre{white-space:pre-wrap;background:#f5f7fa;padding:12px}</style><body>"
        "<h1>T4.1 透明 Composite GPU 与 RenderGraph 验收</h1><p>GPU 回读检查四个 RGBA8 sentinel；Release 场景由有界 runner 启动并自动清理。</p>"
        "<h2>自动检查</h2><table><tr><th>检查</th><th>退出码</th></tr>"
        + rows
        + "</table><h2>真实 GPU scheduler graph</h2><p>场景状态：<strong>"
        + html.escape(record["status"])
        + "</strong>；launcher exit："
        + html.escape(str(record["launcher"]["exit_code"]))
        + "；进程组清空："
        + html.escape(str(record["launcher"]["group_empty"]))
        + "</p><p>"
        + links
        + "</p><pre>"
        + html.escape(json.dumps(graph, indent=2, ensure_ascii=False))
        + "</pre><img src=\""
        + html.escape("captures/" + case + "/output.png")
        + "\" alt=\"T4.1 transparent water scene\"></body></html>"
    )
    target = directory / "final-review.html"
    target.write_text(content, encoding="utf-8")
    return target


def main() -> int:
    run_id = "t4.1-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    directory = ROOT / "tests" / "reports" / run_id
    directory.mkdir(parents=True)
    checks = run_checks(directory)
    if any(check["exit_code"] for check in checks) or len(checks) != len(COMMANDS):
        print(f"T4.1 check failed; report={directory}", file=sys.stderr)
        return 1

    records, cancelled = run(
        ROOT / "tests" / "visual-matrix.json",
        [CASE],
        directory / "captures",
        launcher_binary(ROOT),
        root=ROOT,
        capture_reference=True,
        graph_output=True,
    )
    if cancelled or len(records) != 1:
        return 1
    record = records[0]
    case_directory = directory / "captures" / CASE
    (case_directory / "candidate.json").write_text(
        json.dumps(record, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    if record["status"] != "reference-pending" or record["launcher"]["exit_code"] != 0 or not record["launcher"]["group_empty"]:
        print(f"T4.1 launcher capture failed; report={case_directory}", file=sys.stderr)
        return 1
    graph = validate_graph(case_directory)
    write_report(case_directory, record)
    review = write_review(directory, checks, record, graph)
    print(f"T4.1 report: {review}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
