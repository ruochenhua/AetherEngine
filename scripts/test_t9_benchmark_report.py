#!/usr/bin/env python3
"""Schema and HTML smoke checks for the T9.1 report renderer."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from build_t9_benchmark_report import build, render_report


def fixture() -> tuple[dict, dict]:
    case = {
        "baseline_commit": "0" * 40,
        "samples": 120,
    }
    samples = [
        {
            "frame_index": index,
            "status": {"kind": "Valid"},
            "cpu_extract_ms": 2.0,
            "gpu_pass_ms": None,
            "draw_calls": 45,
            "visible_entities": 1000,
            "memory_bytes": None,
        }
        for index in range(120)
    ]
    series = {
        "schema_version": 1,
        "case_id": "t9_baseline_report",
        "scene": "t9_baseline_report",
        "fixture_source": "fixture",
        "entity_count": 1000,
        "seed": 9001,
        "feature_flags": {"visibility": False, "instancing": False},
        "baseline_commit": case["baseline_commit"],
        "warmup_frames": 30,
        "samples": samples,
        "p50_draw_calls": 45,
        "draw_reduction": None,
        "p50_cpu_ms": 2.0,
        "p95_cpu_ms": 2.0,
        "p50_gpu_ms": None,
        "p95_gpu_ms": None,
        "device": "test",
        "os": "windows",
        "driver": "test driver",
        "adapter": "test adapter",
        "device_key": "windows/x64/test/adapter/1280x720",
        "pair_key": "scene/camera/time/9001/30/120",
        "resolution": [1280, 720],
        "commit": "1" * 40,
        "source_fingerprint": "2" * 64,
        "source_tree_dirty": True,
        "cpu_timer": "CpuInstant",
        "gpu_timer": "Unavailable",
        "rgba8_sha256": "3" * 64,
        "visible_entity_sha256": "4" * 64,
        "state_hash": None,
        "status": "Complete",
    }
    return series, case


class BenchmarkReportTests(unittest.TestCase):
    def test_null_metrics_stay_unavailable_and_dirty_baseline_boundary_is_visible(self) -> None:
        series, case = fixture()
        page = render_report(series, case, image_exists=True)
        self.assertIn("Unavailable (null; timestamp queries were not enabled)", page)
        self.assertIn("not represent the clean pinned baseline", page)
        self.assertIn("<img class=\"capture\"", page)
        self.assertIn("<polyline", page)

    def test_report_writer_produces_reviewable_html(self) -> None:
        series, case = fixture()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output / "benchmark.json").write_text(json.dumps(series), encoding="utf-8")
            (output / "benchmark-case.json").write_text(json.dumps(case), encoding="utf-8")
            self.assertEqual(build(output), 0)
            report = (output / "report.html").read_text(encoding="utf-8")
            self.assertIn("<!doctype html>", report)
            self.assertIn("All per-frame samples", report)
            self.assertEqual((output / "report-status.txt").read_text(encoding="utf-8").strip(), "PASS")


if __name__ == "__main__":
    unittest.main()
