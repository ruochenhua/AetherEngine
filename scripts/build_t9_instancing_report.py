#!/usr/bin/env python3
"""Validate a T9.3 off/on pair and write a visual and performance report."""

from __future__ import annotations

import argparse
import html
import json
import re
from pathlib import Path


HASH_PATTERN = re.compile(r"^[0-9a-f]{64}$")
EXPECTED_ENTITY_COUNT = 10_000
EXPECTED_SEED = 9003
EXPECTED_WARMUP = 30
EXPECTED_SAMPLES = 120
MIN_DRAW_REDUCTION = 0.30


def load_capture(report_dir: Path, variant: str, expected_instancing: bool) -> dict[str, object]:
    case = json.loads((report_dir / f"{variant}.case.json").read_text(encoding="utf-8"))
    series = json.loads((report_dir / f"{variant}.benchmark.json").read_text(encoding="utf-8"))
    visibility = json.loads((report_dir / f"{variant}.visibility.json").read_text(encoding="utf-8"))
    screenshot = report_dir / f"{variant}.png"
    flags = case["feature_flags"]
    if (
        case["entity_count"] != EXPECTED_ENTITY_COUNT
        or case["seed"] != EXPECTED_SEED
        or case["warmup"] != EXPECTED_WARMUP
        or case["samples"] != EXPECTED_SAMPLES
        or flags.get("instancing") is not expected_instancing
        or flags.get("visibility") is not True
        or flags.get("lod") is not False
    ):
        raise ValueError(f"{variant}: case metadata or feature flags do not match the T9.3 contract")
    if (
        series["status"] != "Complete"
        or len(series["samples"]) != EXPECTED_SAMPLES
        or series["entity_count"] != EXPECTED_ENTITY_COUNT
        or series["seed"] != EXPECTED_SEED
        or series["warmup_frames"] != EXPECTED_WARMUP
        or series["feature_flags"] != flags
    ):
        raise ValueError(f"{variant}: benchmark is not a complete 30/120 capture")
    if (
        visibility["schema_version"] != 1
        or visibility["visibility_enabled"] is not True
        or visibility["lod_enabled"] is not False
        or len(visibility["samples"]) != EXPECTED_SAMPLES
    ):
        raise ValueError(f"{variant}: visibility sidecar is incomplete or has unexpected settings")
    for frame_index, (frame, visible) in enumerate(
        zip(series["samples"], visibility["samples"], strict=True)
    ):
        if frame["frame_index"] != frame_index or frame["status"].get("kind") != "Valid":
            raise ValueError(f"{variant}: frame {frame_index} is invalid or out of order")
        if (
            visible["frame_index"] != frame_index
            or visible["visible_entities"] + visible["culled_entities"] != EXPECTED_ENTITY_COUNT
            or visible["visible_entities"] != frame["visible_entities"]
        ):
            raise ValueError(f"{variant}: visibility accounting is inconsistent at frame {frame_index}")
    if not screenshot.is_file() or screenshot.stat().st_size == 0:
        raise ValueError(f"{variant}: screenshot is missing")
    return {
        "case": case,
        "series": series,
        "visibility": visibility,
        "screenshot": screenshot,
    }


def validate_pair(report_dir: Path) -> dict[str, object]:
    baseline = load_capture(report_dir, "baseline", False)
    feature = load_capture(report_dir, "feature", True)
    baseline_case = baseline["case"]
    feature_case = feature["case"]
    baseline_series = baseline["series"]
    feature_series = feature["series"]
    assert isinstance(baseline_case, dict) and isinstance(feature_case, dict)
    assert isinstance(baseline_series, dict) and isinstance(feature_series, dict)

    baseline_flags = dict(baseline_case["feature_flags"])
    feature_flags = dict(feature_case["feature_flags"])
    baseline_flags["instancing"] = feature_flags["instancing"]
    if baseline_flags != feature_flags:
        raise ValueError("baseline and feature cases differ outside the instancing switch")
    for key in (
        "case_id", "scene", "fixture_source", "camera", "render", "time",
        "entity_count", "seed", "warmup", "samples", "baseline_commit",
    ):
        if baseline_case[key] != feature_case[key]:
            raise ValueError(f"paired cases differ in {key}")
    for key in (
        "device_key", "pair_key", "commit", "source_fingerprint",
        "source_tree_dirty", "resolution",
    ):
        if baseline_series[key] != feature_series[key]:
            raise ValueError(f"paired benchmark metadata differs in {key}")
    for key in ("rgba8_sha256", "visible_entity_sha256", "state_hash"):
        left = baseline_series.get(key)
        right = feature_series.get(key)
        if not isinstance(left, str) or not HASH_PATTERN.fullmatch(left):
            raise ValueError(f"baseline {key} is missing or malformed")
        if not isinstance(right, str) or not HASH_PATTERN.fullmatch(right):
            raise ValueError(f"feature {key} is missing or malformed")
        if left != right:
            raise ValueError(f"paired {key} values differ")
    if baseline["visibility"]["samples"] != feature["visibility"]["samples"]:
        raise ValueError("paired visible, culled, and LOD counts differ")
    if any(
        left["visible_entities"] != right["visible_entities"]
        for left, right in zip(
            baseline_series["samples"], feature_series["samples"], strict=True
        )
    ):
        raise ValueError("paired per-frame visible entity counts differ")

    baseline_draws = baseline_series.get("p50_draw_calls")
    feature_draws = feature_series.get("p50_draw_calls")
    if not isinstance(baseline_draws, int) or baseline_draws <= 0:
        raise ValueError("baseline p50 draw calls must be greater than zero")
    if not isinstance(feature_draws, int) or feature_draws < 0:
        raise ValueError("feature p50 draw calls are missing or invalid")
    reduction = (baseline_draws - feature_draws) / baseline_draws
    if reduction < MIN_DRAW_REDUCTION:
        raise ValueError(
            f"draw reduction {reduction:.1%} is below the required {MIN_DRAW_REDUCTION:.0%}"
        )
    for mode, series in (("baseline", baseline_series), ("feature", feature_series)):
        for key in ("p50_cpu_ms", "p95_cpu_ms"):
            if not isinstance(series.get(key), (int, float)):
                raise ValueError(f"{mode} {key} is unavailable")

    return {
        "status": "PASS",
        "gate": "ENABLE_INSTANCING",
        "draw_reduction": reduction,
        "baseline_p50_draw_calls": baseline_draws,
        "feature_p50_draw_calls": feature_draws,
        "hashes": {
            "rgba8_sha256": baseline_series["rgba8_sha256"],
            "visible_entity_sha256": baseline_series["visible_entity_sha256"],
            "state_hash": baseline_series["state_hash"],
        },
        "device_key": baseline_series["device_key"],
        "pair_key": baseline_series["pair_key"],
        "commit": baseline_series["commit"],
        "source_fingerprint": baseline_series["source_fingerprint"],
        "source_tree_dirty": baseline_series["source_tree_dirty"],
        "baseline": {
            "cpu_p50_ms": baseline_series["p50_cpu_ms"],
            "cpu_p95_ms": baseline_series["p95_cpu_ms"],
        },
        "feature": {
            "cpu_p50_ms": feature_series["p50_cpu_ms"],
            "cpu_p95_ms": feature_series["p95_cpu_ms"],
        },
    }


def write_report(report_dir: Path, result: dict[str, object] | None, error: str | None) -> bool:
    passed = result is not None
    delta = result or {
        "status": "FAILED",
        "gate": "KEEP_INSTANCING_DISABLED",
        "failure": error or "paired capture did not meet the T9.3 gate",
    }
    (report_dir / "paired-delta.json").write_text(
        json.dumps(delta, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (report_dir / "report-status.txt").write_text(
        "PASS\n" if passed else "FAILED\n", encoding="utf-8"
    )
    if not passed:
        page = (
            "<!doctype html><html lang='en'><meta charset='utf-8'>"
            "<title>T9.3 paired instancing · FAILED</title>"
            "<body style='font:16px/1.5 system-ui;max-width:900px;margin:2rem auto'>"
            "<h1>T9.3 · Paired instancing failed</h1>"
            f"<p>{html.escape(error or 'paired capture did not meet the gate')}</p>"
            "<p>The production path remains on the individual draw fallback.</p></body></html>"
        )
        (report_dir / "report.html").write_text(page, encoding="utf-8")
        return False

    delta = result
    page = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>T9.3 Paired Instancing · PASS</title><style>
body{{font:15px/1.55 system-ui,sans-serif;max-width:1180px;margin:2rem auto;padding:0 1rem;color:#172033;background:#f8fafc}}
.status{{padding:1rem 1.2rem;border-radius:10px;margin:1rem 0;border-left:5px solid #16803c;background:#effaf2}}
table{{border-collapse:collapse;width:100%;background:#fff}}td,th{{padding:.65rem;border-bottom:1px solid #e4e7ec;text-align:left}}th{{background:#f2f4f7}}
.captures{{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:1rem}}figure{{margin:0;padding:1rem;background:white;border:1px solid #e4e7ec;border-radius:10px}}
img{{display:block;width:100%;height:auto;border:1px solid #cbd5e1}}code{{overflow-wrap:anywhere}}@media(max-width:760px){{.captures{{grid-template-columns:1fr}}}}
</style></head><body><h1>T9.3 · Paired Instancing</h1>
<p class="status"><strong>PASS</strong> · 10,000 entities · 30 warm-up + 120 valid samples per mode · gate enables instancing.</p>
<h2>Paired metrics</h2><table><thead><tr><th>Mode</th><th>Instancing</th><th>Draw calls p50</th><th>CPU extraction p50 / p95</th></tr></thead><tbody>
<tr><td>Baseline</td><td>Off</td><td>{delta['baseline_p50_draw_calls']}</td><td>{delta['baseline']['cpu_p50_ms']:.3f} / {delta['baseline']['cpu_p95_ms']:.3f} ms</td></tr>
<tr><td>Feature</td><td>On</td><td>{delta['feature_p50_draw_calls']}</td><td>{delta['feature']['cpu_p50_ms']:.3f} / {delta['feature']['cpu_p95_ms']:.3f} ms</td></tr>
</tbody></table><p class="status">Draw reduction: <strong>{delta['draw_reduction']:.1%}</strong> (required ≥30%).</p>
<h2>Visual equivalence evidence</h2><p>Decoded RGBA8, visible-entity, and editor-state SHA-256 values match. Per-frame visibility counts and visible entity counts also match.</p>
<div class="captures"><figure><figcaption>Baseline · instancing off</figcaption><img src="baseline.png" alt="Baseline screenshot"></figure>
<figure><figcaption>Feature · instancing on</figcaption><img src="feature.png" alt="Feature screenshot"></figure></div>
<h2>Provenance</h2><p>Device key <code>{html.escape(str(delta['device_key']))}</code> · pair key <code>{html.escape(str(delta['pair_key']))}</code>.</p>
<p>Commit <code>{html.escape(str(delta['commit']))}</code> · source fingerprint <code>{html.escape(str(delta['source_fingerprint']))}</code> · dirty={str(delta['source_tree_dirty']).lower()}.</p>
<p><a href="paired-delta.json">Paired delta JSON</a> · <a href="baseline.benchmark.json">Baseline series</a> · <a href="feature.benchmark.json">Feature series</a></p>
</body></html>"""
    (report_dir / "report.html").write_text(page, encoding="utf-8")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", required=True, type=Path)
    args = parser.parse_args()
    try:
        result = validate_pair(args.report_dir)
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        write_report(args.report_dir, None, str(error))
        return 1
    return 0 if write_report(args.report_dir, result, None) else 1


if __name__ == "__main__":
    raise SystemExit(main())
