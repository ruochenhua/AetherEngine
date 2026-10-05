#!/usr/bin/env python3
"""Focused contract tests for the T9.4 research report builder."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from build_t9_research_report import build_report, validate_occlusion


def prototype(status: str = "Complete") -> dict[str, object]:
    totals = [8] * 928 + [7] * 368
    visible = [6] * 928 + [5] * 368
    occlusion: dict[str, object] = {
        "status": status,
        "candidate_count": 10_000,
        "visible_count": sum(visible),
        "culled_count": 10_000 - sum(visible),
        "candidate_reduction": 1.0 - sum(visible) / 10_000,
        "cpu_gpu_mask_mismatches": 0 if status == "Complete" else None,
        "warmup": 30,
        "samples": 120,
        "gpu_samples_ms": [0.05 + (index % 7) * 0.001 for index in range(120)] if status == "Complete" else [],
        "tile_width": 48,
        "tile_height": 27,
        "tile_totals": totals,
        "tile_visible": visible,
        "known_gpu_buffer_bytes": 170_144 if status == "Complete" else None,
        "unavailable_reason": "timestamp queries unsupported" if status == "Unavailable" else None,
    }
    return {
        "schema_version": 1,
        "research_only": True,
        "prototype_enabled": True,
        "kill_switch": {"production_path_registered": False, "default": "off", "scope": "example-only"},
        "occlusion": occlusion,
    }


class T9ResearchReportTests(unittest.TestCase):
    def test_rejects_gpu_correctness_mismatch(self) -> None:
        data = prototype()
        data["occlusion"]["cpu_gpu_mask_mismatches"] = 1  # type: ignore[index]
        with self.assertRaisesRegex(ValueError, "correctness comparison"):
            validate_occlusion(data)

    def test_accepts_explicit_unavailable_timer_without_fabricating_zero(self) -> None:
        result = validate_occlusion(prototype("Unavailable"))
        self.assertIsNone(result["known_gpu_buffer_bytes"])
        self.assertEqual(result["gpu_samples_ms"], [])

    def test_builds_research_pass_and_production_no_go(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_dir = Path(temp)
            (report_dir / "fixture.ron").write_text("fixture", encoding="utf-8")
            (report_dir / "occlusion-prototype.json").write_text(
                json.dumps(prototype()), encoding="utf-8"
            )
            result = build_report(report_dir, {"commit": "abc123", "dirty": True})
            self.assertEqual(result["aggregate"]["status"], "PASS")
            self.assertEqual(result["prototype"]["production_decision"], "NO_GO_PRODUCTION")
            self.assertAlmostEqual(result["metrics"]["gpu_cost_p50_ms"], 0.053)
            for artifact in (
                "report.html", "research-rfc.html", "prototype.json", "metrics.json",
                "aggregate.json", "output.png", "reference.png", "diff.png", "graph.txt", "launcher.log",
            ):
                self.assertTrue((report_dir / artifact).is_file(), artifact)
            self.assertIn("NO-GO", (report_dir / "report.html").read_text(encoding="utf-8"))

    def test_streaming_memory_remains_labeled_as_a_model(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_dir = Path(temp)
            (report_dir / "fixture.ron").write_text("fixture", encoding="utf-8")
            (report_dir / "occlusion-prototype.json").write_text(
                json.dumps(prototype("Unavailable")), encoding="utf-8"
            )
            result = build_report(report_dir)
            streaming = result["prototype"]["terrain_streaming"]
            self.assertEqual(streaming["status"], "Modeled")
            self.assertIsNone(streaming["measured_gpu_upload_ms"])
            self.assertIn("no engine allocator", streaming["memory_basis"])


if __name__ == "__main__":
    unittest.main()
