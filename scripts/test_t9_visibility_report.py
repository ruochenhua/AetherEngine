#!/usr/bin/env python3
"""Unit tests for the T9.2 1k/10k aggregate report."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from build_t9_visibility_report import build_report


class VisibilityReportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.report_dir = Path(self.temp.name)
        (self.report_dir / "visibility-unit-tests.stdout").write_text(
            "test renderer::visibility::tests::lod_boundaries_select_three_detail_buckets ... ok\n"
            "test renderer::visibility::tests::lod_hysteresis_holds_within_ten_percent_bands ... ok\n"
            "test renderer::visibility::tests::camera_move_changes_visible_and_culled_entity_counts ... ok\n"
            "test renderer::visibility::tests::aabb_tangent_to_frustum_boundary_remains_visible ... ok\n"
            "test result: ok. 7 passed; 0 failed\n",
            encoding="utf-8",
        )
        self.write_variants()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_variants(self, invalid_sum: bool = False) -> None:
        for name, count in (("entities_1k", 1000), ("entities_10k", 10000)):
            visible = count - 100
            (self.report_dir / f"{name}.case.json").write_text(
                json.dumps({"entity_count": count}), encoding="utf-8"
            )
            benchmark_samples = [
                {"status": {"kind": "Valid"}} for _ in range(120)
            ]
            series = {
                "entity_count": count,
                "status": "Complete",
                "samples": benchmark_samples,
                "p50_cpu_ms": 0.1,
                "p95_cpu_ms": 0.2,
                "p50_draw_calls": 4,
                "commit": "7c6792825add554a8ded1eb16e6355ccda31904f",
                "source_fingerprint": "a" * 64,
                "source_tree_dirty": True,
            }
            for sample in benchmark_samples:
                sample["visible_entities"] = visible
            (self.report_dir / f"{name}.benchmark.json").write_text(
                json.dumps(series), encoding="utf-8"
            )
            culled = 99 if invalid_sum and name == "entities_1k" else count - visible
            visibility = {
                "visibility_enabled": True,
                "lod_enabled": True,
                "samples": [
                    {
                        "visible_entities": visible,
                        "culled_entities": culled,
                        "lod_high": 20,
                        "lod_medium": visible - 40,
                        "lod_low": 20,
                    }
                    for _ in range(120)
                ],
            }
            (self.report_dir / f"{name}.visibility.json").write_text(
                json.dumps(visibility), encoding="utf-8"
            )
            (self.report_dir / f"{name}.png").write_bytes(b"png")

    def test_report_includes_both_complete_variants_and_probes(self) -> None:
        self.assertTrue(build_report(self.report_dir))
        page = (self.report_dir / "report.html").read_text(encoding="utf-8")
        self.assertIn("entities_1k", page)
        self.assertIn("entities_10k", page)
        self.assertIn("10% hysteresis PASS", page)
        self.assertEqual(
            (self.report_dir / "report-status.txt").read_text(encoding="utf-8").strip(),
            "PASS",
        )

    def test_report_rejects_visibility_counts_that_do_not_cover_the_fixture(self) -> None:
        self.write_variants(invalid_sum=True)
        self.assertFalse(build_report(self.report_dir))
        self.assertIn(
            "visibility counts do not cover",
            (self.report_dir / "report.html").read_text(encoding="utf-8"),
        )

    def test_report_rejects_missing_lod_coverage(self) -> None:
        visibility_path = self.report_dir / "entities_1k.visibility.json"
        visibility = json.loads(visibility_path.read_text(encoding="utf-8"))
        for sample in visibility["samples"]:
            sample["lod_low"] = 0
            sample["lod_medium"] += 20
        visibility_path.write_text(json.dumps(visibility), encoding="utf-8")
        self.assertFalse(build_report(self.report_dir))
        self.assertIn(
            "did not exercise all three LOD buckets",
            (self.report_dir / "report.html").read_text(encoding="utf-8"),
        )


if __name__ == "__main__":
    unittest.main()
