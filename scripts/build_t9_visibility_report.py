#!/usr/bin/env python3
"""Validate paired 1k/10k T9.2 captures and render a reviewable HTML report."""

from __future__ import annotations

import argparse
import html
import json
import statistics
from pathlib import Path


PINNED_BASELINE = "0725ce0a36e25de4ea9f292208d44e4989cd4668"
VARIANTS = (("entities_1k", 1000), ("entities_10k", 10000))


def median(values: list[int]) -> int | float:
    return statistics.median(values)


def load_variant(report_dir: Path, name: str, expected_entities: int) -> dict[str, object]:
    case = json.loads((report_dir / f"{name}.case.json").read_text(encoding="utf-8"))
    series = json.loads((report_dir / f"{name}.benchmark.json").read_text(encoding="utf-8"))
    visibility = json.loads((report_dir / f"{name}.visibility.json").read_text(encoding="utf-8"))
    screenshot = report_dir / f"{name}.png"
    if case["entity_count"] != expected_entities or series["entity_count"] != expected_entities:
        raise ValueError(f"{name}: case entity count does not match {expected_entities}")
    if series["status"] != "Complete" or len(series["samples"]) != 120:
        raise ValueError(f"{name}: benchmark series is not a complete 120-sample capture")
    if (
        len(visibility["samples"]) != 120
        or not visibility["visibility_enabled"]
        or not visibility["lod_enabled"]
    ):
        raise ValueError(f"{name}: visibility sidecar is incomplete or disabled")
    observed_lod = [False, False, False]
    observed_culling = False
    for frame, sample in zip(series["samples"], visibility["samples"], strict=True):
        if frame["status"].get("kind") != "Valid":
            raise ValueError(f"{name}: benchmark contains an invalid sample")
        if sample["visible_entities"] + sample["culled_entities"] != expected_entities:
            raise ValueError(f"{name}: visibility counts do not cover the fixture")
        if frame["visible_entities"] != sample["visible_entities"]:
            raise ValueError(f"{name}: benchmark and visibility visible counts disagree")
        lod_sum = sample["lod_high"] + sample["lod_medium"] + sample["lod_low"]
        if lod_sum != sample["visible_entities"]:
            raise ValueError(f"{name}: LOD counts do not match visible entities")
        for index, level in enumerate(("high", "medium", "low")):
            observed_lod[index] |= sample[f"lod_{level}"] > 0
        observed_culling |= sample["culled_entities"] > 0
    if not all(observed_lod):
        raise ValueError(f"{name}: fixture did not exercise all three LOD buckets")
    if not observed_culling:
        raise ValueError(f"{name}: fixture did not exercise frustum culling")
    if not screenshot.is_file() or screenshot.stat().st_size == 0:
        raise ValueError(f"{name}: scene screenshot is missing")
    visible = [sample["visible_entities"] for sample in visibility["samples"]]
    culled = [sample["culled_entities"] for sample in visibility["samples"]]
    lod = {
        key: [sample[f"lod_{key}"] for sample in visibility["samples"]]
        for key in ("high", "medium", "low")
    }
    return {
        "name": name,
        "entity_count": expected_entities,
        "series": series,
        "visibility": visibility,
        "screenshot": screenshot,
        "visible_p50": median(visible),
        "culled_p50": median(culled),
        "lod_p50": {key: median(values) for key, values in lod.items()},
        "visible_range": [min(visible), max(visible)],
        "culled_range": [min(culled), max(culled)],
    }


def build_report(report_dir: Path) -> bool:
    report_dir.mkdir(parents=True, exist_ok=True)
    status_path = report_dir / "report-status.txt"
    try:
        test_log = (report_dir / "visibility-unit-tests.stdout").read_text(encoding="utf-8")
        required_tests = (
            "lod_boundaries_select_three_detail_buckets",
            "lod_hysteresis_holds_within_ten_percent_bands",
            "camera_move_changes_visible_and_culled_entity_counts",
            "aabb_tangent_to_frustum_boundary_remains_visible",
        )
        missing = [test for test in required_tests if test not in test_log]
        if missing or "test result: ok" not in test_log:
            raise ValueError(f"visibility algorithm tests missing or failed: {', '.join(missing)}")
        variants = [load_variant(report_dir, name, count) for name, count in VARIANTS]
        source = variants[0]["series"]
        if not isinstance(source, dict):
            raise ValueError("benchmark metadata is malformed")
        if any(variant["series"]["commit"] != source["commit"] for variant in variants):
            raise ValueError("1k and 10k variants were captured from different commits")
        if any(
            variant["series"]["source_fingerprint"] != source["source_fingerprint"]
            for variant in variants
        ):
            raise ValueError("1k and 10k variants were captured from different source states")
        if any(
            variant["series"]["source_tree_dirty"] != source["source_tree_dirty"]
            for variant in variants
        ):
            raise ValueError("1k and 10k variants have different dirty-tree provenance")
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        (report_dir / "report.html").write_text(
            "<!doctype html><html lang='en'><meta charset='utf-8'><title>T9.2 failed</title>"
            f"<h1>T9.2 report validation failed</h1><pre>{html.escape(str(error))}</pre>",
            encoding="utf-8",
        )
        status_path.write_text("FAILED\n", encoding="utf-8")
        return False

    commit = str(source["commit"])
    dirty = bool(source["source_tree_dirty"])
    fingerprint = str(source["source_fingerprint"])
    baseline_match = commit == PINNED_BASELINE and not dirty
    notice = (
        "This is the clean pinned baseline."
        if baseline_match
        else "This is a complete current-source profile, not the clean pinned baseline; "
        f"pinned {PINNED_BASELINE}, measured {commit}, dirty={str(dirty).lower()}."
    )
    rows = []
    captures = []
    for item in variants:
        name = str(item["name"])
        series = item["series"]
        lod = item["lod_p50"]
        rows.append(
            "<tr>"
            f"<td>{name}</td><td>{item['entity_count']:,}</td>"
            f"<td>{item['visible_p50']:,.0f}</td><td>{item['culled_p50']:,.0f}</td>"
            f"<td>{lod['high']:,.0f} / {lod['medium']:,.0f} / {lod['low']:,.0f}</td>"
            f"<td>{series['p50_cpu_ms']:.3f} / {series['p95_cpu_ms']:.3f} ms</td>"
            f"<td>{series['p50_draw_calls']}</td></tr>"
        )
        screenshot = str(item["screenshot"])
        captures.append(
            f"<section><h3>{name} · {item['entity_count']:,} entities</h3>"
            f"<p>Visible p50 {item['visible_p50']:,.0f} "
            f"(range {item['visible_range'][0]:,}–{item['visible_range'][1]:,}); "
            f"culled p50 {item['culled_p50']:,.0f} "
            f"(range {item['culled_range'][0]:,}–{item['culled_range'][1]:,}).</p>"
            f"<img src='{html.escape(Path(screenshot).name)}' alt='{name} screenshot'>"
            f"<p><a href='{name}.benchmark.json'>benchmark JSON</a> · "
            f"<a href='{name}.visibility.json'>visibility samples</a></p></section>"
        )
    page = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T9.2 Visibility and LOD · PASS</title><style>
body{{font:15px/1.55 system-ui,sans-serif;max-width:1160px;margin:2rem auto;padding:0 1rem;color:#172033;background:#f8fafc}}
.status,.notice{{padding:1rem 1.2rem;border-radius:10px;margin:1rem 0}}.status{{border-left:5px solid #16803c;background:#effaf2}}
.notice{{border-left:5px solid #b54708;background:#fffaeb}}table{{border-collapse:collapse;width:100%;background:#fff}}td,th{{padding:.6rem;border-bottom:1px solid #e4e7ec;text-align:left}}
th{{background:#f2f4f7}}section{{margin:1.5rem 0;padding:1rem;background:white;border:1px solid #e4e7ec;border-radius:10px}}
img{{display:block;width:100%;height:auto;border:1px solid #cbd5e1}}code{{overflow-wrap:anywhere}}
</style></head><body><h1>T9.2 · Visibility and LOD</h1>
<p class="status"><strong>PASS</strong> · 1k and 10k variants each include 30 warm-up and 120 valid samples.</p>
<p class="notice">{html.escape(notice)}</p>
<h2>Per-variant results</h2><table><thead><tr><th>Variant</th><th>Entities</th><th>Visible p50</th><th>Culled p50</th><th>LOD high / medium / low p50</th><th>CPU extraction p50 / p95</th><th>Draw calls p50</th></tr></thead>
<tbody>{''.join(rows)}</tbody></table>
<p>LOD probes: boundaries PASS; 10% hysteresis PASS; camera movement PASS; frustum-boundary AABB PASS. GPU timestamps and memory remain unavailable as null.</p>
<h2>Scene captures</h2>{''.join(captures)}
<h2>Provenance</h2><p>Commit <code>{html.escape(commit)}</code> · source fingerprint <code>{html.escape(fingerprint)}</code> · dirty={str(dirty).lower()}.</p>
</body></html>"""
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    status_path.write_text("PASS\n", encoding="utf-8")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", required=True, type=Path)
    args = parser.parse_args()
    return 0 if build_report(args.report_dir) else 1


if __name__ == "__main__":
    raise SystemExit(main())
