#!/usr/bin/env python3
"""Contract tests for T9.3 paired report validation and fallback behavior."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_t9_instancing_report as report


HASH_A = "a" * 64
HASH_B = "b" * 64


def case(instancing: bool) -> dict[str, object]:
    return {
        "case_id": "t9_instancing_paired_10k",
        "scene": "t9_instancing_paired_10k",
        "fixture_source": "fixture-sha256",
        "camera": {"source": "scene", "override": None},
        "render": {"width": 1280, "height": 720, "format": "rgba8"},
        "time": {
            "mode": "fixed_step",
            "simulation_time": 0.0,
            "fixed_dt": 0.016666667,
        },
        "entity_count": 10000,
        "seed": 9003,
        "warmup": 30,
        "samples": 120,
        "feature_flags": {
            "instancing": instancing,
            "visibility": True,
            "lod": False,
        },
        "baseline_commit": "0" * 40,
    }


def series(instancing: bool, draw_calls: int, device_key: str = "device") -> dict[str, object]:
    return {
        "status": "Complete",
        "entity_count": 10000,
        "seed": 9003,
        "warmup_frames": 30,
        "feature_flags": case(instancing)["feature_flags"],
        "samples": [
            {
                "frame_index": index,
                "status": {"kind": "Valid"},
                "visible_entities": 500,
            }
            for index in range(120)
        ],
        "device_key": device_key,
        "pair_key": "scene/camera/time/seed/window",
        "commit": "1" * 40,
        "source_fingerprint": "2" * 64,
        "source_tree_dirty": True,
        "resolution": [1280, 720],
        "rgba8_sha256": HASH_A,
        "visible_entity_sha256": HASH_B,
        "state_hash": "c" * 64,
        "p50_draw_calls": draw_calls,
        "p50_cpu_ms": 1.0,
        "p95_cpu_ms": 1.2,
    }


def visibility() -> dict[str, object]:
    return {
        "schema_version": 1,
        "visibility_enabled": True,
        "lod_enabled": False,
        "samples": [
            {
                "frame_index": index,
                "visible_entities": 500,
                "culled_entities": 9500,
            }
            for index in range(120)
        ],
    }


class InstancingReportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.report_dir = Path(self.temp.name)
        self.write_pair()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_json(self, name: str, value: object) -> None:
        (self.report_dir / name).write_text(json.dumps(value), encoding="utf-8")

    def write_pair(self, baseline_draws: int = 100, feature_draws: int = 50) -> None:
        self.write_json("baseline.case.json", case(False))
        self.write_json("feature.case.json", case(True))
        self.write_json("baseline.benchmark.json", series(False, baseline_draws))
        self.write_json("feature.benchmark.json", series(True, feature_draws))
        self.write_json("baseline.visibility.json", visibility())
        self.write_json("feature.visibility.json", visibility())
        (self.report_dir / "baseline.png").write_bytes(b"baseline")
        (self.report_dir / "feature.png").write_bytes(b"feature")

    def test_accepts_complete_equivalent_pair_above_threshold(self) -> None:
        delta = report.validate_pair(self.report_dir)
        self.assertEqual(delta["status"], "PASS")
        self.assertEqual(delta["gate"], "ENABLE_INSTANCING")
        self.assertEqual(delta["draw_reduction"], 0.5)

    def test_rejects_draw_reduction_below_threshold(self) -> None:
        self.write_pair(baseline_draws=100, feature_draws=71)
        with self.assertRaisesRegex(ValueError, "below the required 30%"):
            report.validate_pair(self.report_dir)

    def test_rejects_device_mismatch(self) -> None:
        feature = series(True, 50, device_key="other-device")
        self.write_json("feature.benchmark.json", feature)
        with self.assertRaisesRegex(ValueError, "device_key"):
            report.validate_pair(self.report_dir)

    def test_rejects_visual_hash_mismatch(self) -> None:
        feature = series(True, 50)
        feature["rgba8_sha256"] = "d" * 64
        self.write_json("feature.benchmark.json", feature)
        with self.assertRaisesRegex(ValueError, "rgba8_sha256"):
            report.validate_pair(self.report_dir)

    def test_rejects_editor_state_hash_mismatch(self) -> None:
        feature = series(True, 50)
        feature["state_hash"] = "d" * 64
        self.write_json("feature.benchmark.json", feature)
        with self.assertRaisesRegex(ValueError, "state_hash"):
            report.validate_pair(self.report_dir)


if __name__ == "__main__":
    unittest.main(verbosity=2)
