#!/usr/bin/env python3
"""Tests for machine-readable roadmap status synchronization."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SYNC = ROOT / "scripts" / "sync_roadmap_status.py"


class RoadmapStatusSyncTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "docs").mkdir()
        roadmap = self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap"
        roadmap.mkdir(parents=True)
        (self.root / "docs/evidence.md").write_text("Tracked evidence\n", encoding="utf-8")
        (roadmap / "t0-t6-status-audit.md").write_text(
            "<!-- ROADMAP_ARCHIVED: 2026-10-05 -->\n历史档案。Current status: [verification](verification-current-20261011.md)\n",
            encoding="utf-8",
        )
        self.manifest = {
            "schema_version": 1,
            "updated": "2026-10-11",
            "tasks": [
                {
                    "id": f"T{task_number}",
                    "title": "Verification foundation" if task_number == 0 else f"Task {task_number}",
                    "title_zh": "验证基础" if task_number == 0 else f"任务 {task_number}",
                    "implementation": "complete",
                    "automated": "passed",
                    "acceptance": "pending",
                    "decision": None,
                    "remaining": ["Review the candidate images."] if task_number == 0 else [],
                    "remaining_zh": ["复核候选图。"] if task_number == 0 else [],
                    "evidence": [{"label": "Details", "label_zh": "详情", "path": "docs/evidence.md"}],
                }
                for task_number in range(10)
            ],
        }
        (roadmap / "roadmap-status.json").write_text(
            json.dumps(self.manifest, indent=2) + "\n", encoding="utf-8"
        )
        for path, start, end in (
            ("README.md", "<!-- ROADMAP_STATUS:START -->", "<!-- ROADMAP_STATUS:END -->"),
            ("README.zh-CN.md", "<!-- ROADMAP_STATUS_ZH:START -->", "<!-- ROADMAP_STATUS_ZH:END -->"),
            ("docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html", "<!-- ROADMAP_STATUS:START -->", "<!-- ROADMAP_STATUS:END -->"),
            ("docs/plans/2026-09-17-non-raytracing-engine-roadmap/verification-current-20261011.md", "<!-- ROADMAP_STATUS:START -->", "<!-- ROADMAP_STATUS:END -->"),
        ):
            (self.root / path).write_text(f"Before\n{start}\nold\n{end}\nAfter\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "docs/evidence.md", str(roadmap / "t0-t6-status-audit.md")], check=True)

    def tearDown(self):
        self.temp.cleanup()

    def run_sync(self, mode):
        return subprocess.run(
            [sys.executable, str(SYNC), mode, "--root", str(self.root)],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_write_generates_all_views_and_check_accepts_them(self):
        result = self.run_sync("--write")
        self.assertEqual(result.returncode, 0, result.stderr)
        readme = (self.root / "README.md").read_text(encoding="utf-8")
        self.assertIn("T0", readme)
        self.assertIn("Review the candidate images.", readme)
        self.assertIn("docs/evidence.md", readme)
        self.assertIn("T9", (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/index.html").read_text(encoding="utf-8"))
        self.assertIn("verification foundation", readme.lower())
        self.assertIn("<!-- ROADMAP_STATUS_ZH:START -->", (self.root / "README.zh-CN.md").read_text(encoding="utf-8"))
        chinese = (self.root / "README.zh-CN.md").read_text(encoding="utf-8")
        self.assertIn("验证基础", chinese)
        self.assertIn("复核候选图。", chinese)
        self.assertNotIn("Verification foundation", chinese)
        current = (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/verification-current-20261011.md").read_text(encoding="utf-8")
        self.assertIn("[Details](../../evidence.md)", current)
        self.assertEqual(self.run_sync("--check").returncode, 0)

    def test_check_fails_when_generated_block_is_stale(self):
        self.assertNotEqual(self.run_sync("--check").returncode, 0)

    def test_check_detects_drift_after_generation(self):
        self.assertEqual(self.run_sync("--write").returncode, 0)
        path = self.root / "README.md"
        path.write_text(path.read_text(encoding="utf-8").replace("Review the candidate images.", "Changed generated status."), encoding="utf-8")
        result = self.run_sync("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("README.md", result.stderr)

    def test_rejects_duplicate_task_ids(self):
        self.manifest["tasks"].append(dict(self.manifest["tasks"][0]))
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate", result.stderr.lower())

    def test_rejects_unknown_status_values(self):
        self.manifest["tasks"][0]["automated"] = "looks-good"
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("automated", result.stderr)

    def test_rejects_missing_or_ignored_evidence_links(self):
        self.manifest["tasks"][0]["evidence"][0]["path"] = "missing.md"
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("evidence", result.stderr.lower())

    def test_rejects_ignored_report_paths_even_if_tracked(self):
        reports = self.root / "tests/reports"
        reports.mkdir(parents=True)
        report = reports / "local-report.html"
        report.write_text("local artifact\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(self.root), "add", "-f", "tests/reports/local-report.html"], check=True)
        self.manifest["tasks"][0]["evidence"][0]["path"] = "tests/reports/local-report.html"
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ignored run output", result.stderr)

    def test_rejects_evidence_not_tracked_by_git(self):
        (self.root / "docs/new-evidence.md").write_text("Not tracked\n", encoding="utf-8")
        self.manifest["tasks"][0]["evidence"][0]["path"] = "docs/new-evidence.md"
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("tracked", result.stderr.lower())

    def test_check_requires_old_audit_to_be_marked_archived(self):
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/t0-t6-status-audit.md").write_text("Old status notes\n", encoding="utf-8")
        result = self.run_sync("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("archive", result.stderr.lower())

    def test_rejects_incomplete_t0_to_t9_register(self):
        self.manifest["tasks"].pop()
        (self.root / "docs/plans/2026-09-17-non-raytracing-engine-roadmap/roadmap-status.json").write_text(
            json.dumps(self.manifest), encoding="utf-8"
        )
        result = self.run_sync("--write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("T0–T9", result.stderr)


if __name__ == "__main__":
    unittest.main()
