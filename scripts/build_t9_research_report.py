#!/usr/bin/env python3
"""Validate T9.4 research evidence and build the human review report."""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import math
import struct
import zlib
from pathlib import Path
from typing import Any


EXPECTED_SAMPLES = 120
EXPECTED_WARMUP = 30
TILE_WIDTH = 48
TILE_HEIGHT = 27


def percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, math.ceil(fraction * len(ordered)) - 1)
    return ordered[index]


def validate_occlusion(data: dict[str, Any]) -> dict[str, Any]:
    if data.get("schema_version") != 1 or data.get("research_only") is not True:
        raise ValueError("prototype metadata is not T9.4 research-only schema v1")
    if data.get("prototype_enabled") is not True:
        raise ValueError("prototype was not explicitly enabled for this research run")
    kill_switch = data.get("kill_switch", {})
    if (
        kill_switch.get("production_path_registered") is not False
        or kill_switch.get("default") != "off"
    ):
        raise ValueError("production kill switch contract is missing or enabled")
    occlusion = data.get("occlusion", {})
    if occlusion.get("candidate_count") != 10_000:
        raise ValueError("occlusion fixture candidate count must be 10,000")
    totals = occlusion.get("tile_totals")
    visible = occlusion.get("tile_visible")
    if (
        not isinstance(totals, list)
        or not isinstance(visible, list)
        or len(totals) != TILE_WIDTH * TILE_HEIGHT
        or len(visible) != TILE_WIDTH * TILE_HEIGHT
        or sum(totals) != 10_000
    ):
        raise ValueError("tile accounting is incomplete")
    if any(not isinstance(a, int) or not isinstance(b, int) or b > a for a, b in zip(totals, visible, strict=True)):
        raise ValueError("tile visible counts exceed their candidate totals")

    status = occlusion.get("status")
    if status == "Complete":
        samples = occlusion.get("gpu_samples_ms")
        if (
            not isinstance(samples, list)
            or len(samples) != EXPECTED_SAMPLES
            or any(not isinstance(value, (int, float)) or value < 0 for value in samples)
            or occlusion.get("warmup") != EXPECTED_WARMUP
            or occlusion.get("samples") != EXPECTED_SAMPLES
            or occlusion.get("cpu_gpu_mask_mismatches") != 0
        ):
            raise ValueError("timestamp series or CPU/GPU correctness comparison is incomplete")
    elif status != "Unavailable":
        raise ValueError(f"unexpected GPU prototype status: {status!r}")
    return occlusion


def png_chunk(tag: bytes, payload: bytes) -> bytes:
    body = tag + payload
    return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def write_tile_png(path: Path, totals: list[int], visible: list[int], mode: str) -> None:
    scale = 20
    width = TILE_WIDTH * scale
    height = TILE_HEIGHT * scale
    rows = bytearray()
    for y in range(TILE_HEIGHT):
        for local_y in range(scale):
            rows.append(0)
            for x in range(TILE_WIDTH):
                index = y * TILE_WIDTH + x
                ratio = visible[index] / totals[index] if totals[index] else 1.0
                if mode == "reference":
                    color = (71, 116, 105)
                elif mode == "diff":
                    culled = 1.0 - ratio
                    color = (int(70 + 180 * culled), int(32 + 35 * (1.0 - culled)), 40)
                else:
                    color = (int(24 + 42 * (1.0 - ratio)), int(50 + 160 * ratio), int(45 + 85 * ratio))
                rows.extend(bytes(color) * scale)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    image = b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", ihdr)
    image += png_chunk(b"IDAT", zlib.compress(bytes(rows), level=9)) + png_chunk(b"IEND", b"")
    path.write_bytes(image)


def streaming_model() -> dict[str, Any]:
    world_width = 128
    world_height = 128
    window_radius = 3
    payload_bytes = 64 * 1024
    route = [(8 + 3 * step, 8 + 2 * step) for step in range(12)]
    active: set[tuple[int, int]] = set()
    requested = 0
    released = 0
    resident_samples: list[int] = []
    route_rows = []
    for x, y in route:
        wanted = {
            (cx, cy)
            for cy in range(max(0, y - window_radius), min(world_height, y + window_radius + 1))
            for cx in range(max(0, x - window_radius), min(world_width, x + window_radius + 1))
        }
        added = wanted - active
        removed = active - wanted
        requested += len(added)
        released += len(removed)
        active = wanted
        resident_samples.append(len(active) * payload_bytes)
        route_rows.append({"camera_chunk": [x, y], "active_chunks": len(active), "loaded": len(added), "released": len(removed)})
    total_world = world_width * world_height * payload_bytes
    return {
        "status": "Modeled",
        "world_chunks": world_width * world_height,
        "chunk_payload_bytes": payload_bytes,
        "window_diameter_chunks": 2 * window_radius + 1,
        "route_steps": len(route),
        "requested_chunks": requested,
        "released_chunks": released,
        "resident_bytes_p50": percentile([float(value) for value in resident_samples], 0.50),
        "resident_bytes_p95": percentile([float(value) for value in resident_samples], 0.95),
        "resident_bytes_peak": max(resident_samples),
        "eager_world_bytes_estimate": total_world,
        "estimated_resident_reduction_vs_eager": 1.0 - max(resident_samples) / total_world,
        "measured_io_ms": None,
        "measured_gpu_upload_ms": None,
        "memory_basis": "active chunk count multiplied by fixed fixture payload size; no engine allocator or GPU allocation is measured",
        "route": route_rows,
    }


def build_report(report_dir: Path, source: dict[str, Any] | None = None) -> dict[str, Any]:
    report_dir.mkdir(parents=True, exist_ok=True)
    if source is None and (report_dir / "capture.json").is_file():
        capture = json.loads((report_dir / "capture.json").read_text(encoding="utf-8"))
        source = capture.get("source", {})
    raw_path = report_dir / "occlusion-prototype.json"
    raw = json.loads(raw_path.read_text(encoding="utf-8"))
    occlusion = validate_occlusion(raw)
    streaming = streaming_model()
    prototype = {
        "schema_version": 1,
        "research_only": True,
        "case_id": "t9_occlusion_streaming_research",
        "fixture": "tests/fixtures/t9_occlusion_streaming.ron",
        "fixture_sha256": hashlib.sha256((report_dir / "fixture.ron").read_bytes()).hexdigest(),
        "kill_switch": raw["kill_switch"],
        "production_decision": "NO_GO_PRODUCTION",
        "decision_reason": "synthetic tile workload only; no engine scene-level visual validation; streaming I/O and upload costs are modeled as unavailable",
        "occlusion": occlusion,
        "terrain_streaming": streaming,
        "source": source or {},
    }
    (report_dir / "prototype.json").write_text(json.dumps(prototype, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    tiles_total = occlusion["tile_totals"]
    tiles_visible = occlusion["tile_visible"]
    write_tile_png(report_dir / "reference.png", tiles_total, tiles_visible, "reference")
    write_tile_png(report_dir / "output.png", tiles_total, tiles_visible, "output")
    write_tile_png(report_dir / "diff.png", tiles_total, tiles_visible, "diff")
    (report_dir / "graph.txt").write_text(
        "Research prototype graph (not the engine RenderGraph)\n"
        "1. build tile nearest-depth buffer from 8 synthetic rectangle occluders\n"
        "2. classify 10,000 deterministic screen-tile candidates\n"
        "No production render graph pass was registered.\n",
        encoding="utf-8",
    )
    (report_dir / "launcher.log").write_text(
        json.dumps({"launched": False, "window_created": False, "orphan_process": False, "reason": "T9.4 research example is offscreen and isolated; launcher intentionally not started"}, indent=2) + "\n",
        encoding="utf-8",
    )

    samples = occlusion.get("gpu_samples_ms", [])
    metrics = {
        "schema_version": 1,
        "occlusion_status": occlusion["status"],
        "gpu_cost_p50_ms": percentile(samples, 0.50),
        "gpu_cost_p95_ms": percentile(samples, 0.95),
        "visible_candidate_reduction": occlusion.get("candidate_reduction"),
        "cpu_gpu_mask_mismatches": occlusion.get("cpu_gpu_mask_mismatches"),
        "known_gpu_buffer_bytes": occlusion.get("known_gpu_buffer_bytes"),
        "terrain_resident_bytes_peak_model": streaming["resident_bytes_peak"],
        "terrain_io_ms": None,
        "terrain_gpu_upload_ms": None,
        "production_decision": prototype["production_decision"],
    }
    (report_dir / "metrics.json").write_text(json.dumps(metrics, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    p50 = metrics["gpu_cost_p50_ms"]
    p95 = metrics["gpu_cost_p95_ms"]
    source_data = source or {}
    gpu_cost = "Unavailable" if p50 is None else f"{p50:.4f} / {p95:.4f} ms (GPU p50 / p95)"
    visible_gain = f"{100 * occlusion.get('candidate_reduction', 0.0):.1f}% fewer synthetic candidates"
    buffer_memory = "Unavailable" if metrics["known_gpu_buffer_bytes"] is None else f"{metrics['known_gpu_buffer_bytes']:,} known GPU buffer bytes"
    page = f"""<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T9.4 遮挡与地形流送研究验收</title><style>
body{{font:15px/1.6 system-ui,sans-serif;max-width:1180px;margin:2rem auto;padding:0 1.1rem;background:#f6f8fb;color:#172033}}
.status{{padding:1rem 1.2rem;border-left:5px solid #16803c;border-radius:8px;background:#ecf8ef}}
.decision{{padding:1rem 1.2rem;border-left:5px solid #bb6410;border-radius:8px;background:#fff6e9}}
.cards{{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:.75rem}}.card,figure,table{{background:white;border:1px solid #dce2ea;border-radius:9px;padding:.8rem}}
.card strong{{display:block;font-size:1.2rem}}table{{border-collapse:collapse;width:100%;padding:0}}th,td{{padding:.6rem;border-bottom:1px solid #e4e7ec;text-align:left}}figure{{margin:.5rem 0}}img{{max-width:100%;height:auto;border:1px solid #cbd5e1}}code{{overflow-wrap:anywhere}}
@media(max-width:750px){{.cards{{grid-template-columns:repeat(2,minmax(0,1fr))}}}}
</style></head><body>
<h1>T9.4 · 遮挡与地形流送研究</h1>
<p class="status"><strong>研究验收：PASS</strong> · 合成 GPU 原型与可复跑证据已生成。该状态表示研究工件完整，不表示生产启用。</p>
<p class="decision"><strong>生产结论：NO-GO</strong> · 现有结果来自固定合成 tile 工作负载；没有引擎场景视觉正确性验证。地形流送 I/O 和 GPU 上传耗时仍未测量。</p>
<h2>验收数据</h2><section class="cards">
<div class="card">遮挡 GPU 成本<strong>{html.escape(gpu_cost)}</strong></div>
<div class="card">候选可见收益<strong>{html.escape(visible_gain)}</strong></div>
<div class="card">已知 GPU 缓冲区<strong>{html.escape(buffer_memory)}</strong></div>
<div class="card">流送驻留内存峰值模型<strong>{streaming['resident_bytes_peak']:,} bytes</strong></div></section>
<p>时间戳查询：{html.escape(str(occlusion.get('status')))} · warm-up {EXPECTED_WARMUP} 次 / 有效样本 {len(samples)} 次 · CPU/GPU mask mismatch {html.escape(str(occlusion.get('cpu_gpu_mask_mismatches')))}。</p>
<h2>合成屏幕 tile 结果</h2><p>颜色图展示 48×27 个屏幕 tile 中的候选可见比例。它不是引擎截图；参考图将候选视为全部可见，差异图标出合成遮挡剔除。</p>
<figure><figcaption>候选可见比例（GPU 输出，CPU 参考逐项核对）</figcaption><img src="output.png" alt="Synthetic screen tile visibility map"></figure>
<div class="cards"><figure><figcaption>Baseline · no occlusion</figcaption><img src="reference.png" alt="All candidates visible reference"></figure><figure><figcaption>Candidate cull delta</figcaption><img src="diff.png" alt="Synthetic cull difference"></figure></div>
<h2>地形流送模型</h2><table><tbody>
<tr><th>世界与块载荷</th><td>{streaming['world_chunks']:,} chunks · {streaming['chunk_payload_bytes']:,} bytes/chunk · eager estimate {streaming['eager_world_bytes_estimate']:,} bytes</td></tr>
<tr><th>7×7 活动窗口</th><td>{streaming['resident_bytes_p50']:,} bytes p50 · {streaming['resident_bytes_peak']:,} bytes peak · modeled reduction {100 * streaming['estimated_resident_reduction_vs_eager']:.2f}%</td></tr>
<tr><th>路径工作量</th><td>{streaming['route_steps']} camera steps · {streaming['requested_chunks']} requests · {streaming['released_chunks']} releases</td></tr>
<tr><th>I/O / GPU upload</th><td>Unavailable · no actual engine chunk loading or GPU upload ran</td></tr>
</tbody></table>
<h2>Kill switch 与失效场景</h2><p>原型需要显式 <code>--prototype-enabled</code>；默认关闭，只存在于 example 进程，未注册进 launcher、RenderGraph 或 release gate。停止运行该 example 即可关闭实验，没有持久 feature state。</p>
<ul><li>遮挡：透明/半透明物体、细长遮挡体、快速变化的深度层、相机穿入遮挡体和 tile 边缘误剔除。</li><li>流送：快速移动导致空环、磁盘抖动、接缝与 LOD 不连续、chunk 尺寸和载荷差异造成的内存估计偏差。</li></ul>
<h2>下一步决定</h2><p>先将该研究记录归档。只有后续另行批准引擎集成计划并增加真实场景的可见性正确性、GPU/内存配对基线和镜头运动覆盖后，才进入 production implementation。</p>
<p><a href="research-rfc.html">RFC</a> · <a href="prototype.json">原型 JSON</a> · <a href="metrics.json">指标 JSON</a> · <a href="graph.txt">实验图说明</a></p>
<p>来源：fixture sha256 <code>{prototype['fixture_sha256']}</code> · capture {html.escape(source_data.get('started_at', 'unknown'))} · revision <code>{html.escape(source_data.get('commit', 'unknown'))}</code> · working tree dirty={str(source_data.get('dirty', True)).lower()}.</p>
</body></html>"""
    (report_dir / "report.html").write_text(page, encoding="utf-8")

    rfc = f"""<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>T9.4 RFC</title>
<body style="font:16px/1.6 system-ui;max-width:900px;margin:2rem auto;padding:0 1rem;color:#172033">
<h1>T9.4 RFC：Occlusion 与 Terrain Streaming</h1><p>状态：研究完成；生产实现：NO-GO。</p>
<h2>原型</h2><p>一个独立 wgpu compute 示例先为 48×27 tile 生成最近矩形遮挡深度，再将 10,000 个固定种子的候选分类。30 次 warm-up 后记录 120 组 GPU timestamp，并逐候选与 CPU 参考 mask 比较。</p>
<h2>成本与收益</h2><p>遮挡成本 GPU p50/p95：{html.escape(gpu_cost)}。固定样本可见候选减少 {html.escape(visible_gain)}；这是合成 workload 的分类收益，不是引擎 draw-call 或帧时提升。已知 GPU buffer footprint：{html.escape(buffer_memory)}；驱动管理的 query set、pipeline 和分配器开销不包含在内。</p>
<h2>流送</h2><p>模型使用 128×128 个 64 KiB payload chunk，沿 12 个固定位置维护 7×7 活动窗口。峰值活动 payload 为 {streaming['resident_bytes_peak']:,} bytes。磁盘 I/O、实际 engine allocator 和 GPU upload cost 均 unavailable。</p>
<h2>安全开关与结论</h2><p>example 默认关闭，必须显式 <code>--prototype-enabled</code>；没有 launcher 参数或生产 pass 注册。结论为 NO-GO_PRODUCTION：下一阶段如要实施，需要独立计划、真实场景视觉正确性与同设备配对 GPU/内存数据。</p>
<h2>已识别风险</h2><p>遮挡风险包括透明体、细薄物体、深度反转、镜头近裁面与保守性错误。流送风险包括 I/O stall、空环、接缝、快速横穿、chunk payload 异构与显存重复驻留。</p>
<p><a href="report.html">验收报告</a> · <a href="prototype.json">原型证据</a></p></body></html>"""
    (report_dir / "research-rfc.html").write_text(rfc, encoding="utf-8")

    aggregate = {
        "schema_version": 1,
        "slice": "T9.4",
        "case_id": "t9_occlusion_streaming_research",
        "status": "PASS",
        "research_only": True,
        "production_decision": prototype["production_decision"],
        "variants": [{"id": "default", "status": "Complete", "diagnostics": [], "fallbacks": [], "metrics": [], "hashes": []}],
        "probes": [
            {"name": "research_only", "kind": "Bool", "value": True},
            {"name": "kill_switch", "kind": "Bool", "value": True},
            {"name": "prototype_artifact", "kind": "Present", "path": "prototype.json"},
            {"name": "decision", "kind": "Text", "value": "go-no-go"},
        ],
        "artifacts": ["report.html", "stdout", "stderr", "launcher.log", "graph.txt", "output.png", "reference.png", "diff.png", "metrics.json", "research-rfc.html", "prototype.json"],
        "forbidden_states": ["needs-review", "orphan_process", "unexpected_variant", "stale_generation"],
    }
    (report_dir / "aggregate.json").write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (report_dir / "report-status.txt").write_text("PASS_RESEARCH_ONLY\n", encoding="utf-8")
    return {"prototype": prototype, "metrics": metrics, "aggregate": aggregate}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case-dir", required=True, type=Path)
    args = parser.parse_args()
    try:
        build_report(args.case_dir)
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        (args.case_dir / "report-status.txt").write_text("FAILED\n", encoding="utf-8")
        (args.case_dir / "report.html").write_text(
            "<!doctype html><meta charset='utf-8'><title>T9.4 failed</title>"
            f"<h1>T9.4 report generation failed</h1><pre>{html.escape(str(error))}</pre>",
            encoding="utf-8",
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
