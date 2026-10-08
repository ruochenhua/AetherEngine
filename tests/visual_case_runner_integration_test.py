"""End-to-end fake-launcher check for the v2 serial runner and aggregate artifacts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts/run_visual_case_v2.py"

FAKE_LAUNCHER = r'''#!PYTHON
import json, os, sys
from pathlib import Path
args = sys.argv
output = Path(args[args.index('--screenshot') + 1])
output.write_bytes(b'png')
(output.parent / 'outcome.json').write_text(json.dumps({
    'kind': 'Render', 'diagnostics': [], 'fallbacks': [], 'metrics': {}, 'hashes': {},
    'probes': {'initial_load_commit': True, 'cancel_idempotent': True,
               'worker_gpu_commit': False, 'state_machine': 'valid'},
    'asset_events': [{'event': 'loaded', 'generation': 1}], 'graph': None
}))
os.write(int(os.environ['AETHER_READY_FD']), b'R')
'''


class VisualCaseRunnerIntegrationTests(unittest.TestCase):
    def test_variant_outcomes_and_asset_events_are_written_to_aggregate(self):
        with tempfile.TemporaryDirectory(prefix="aether-v2-runner-") as temp:
            root = Path(temp)
            launcher = root / "fake launcher"
            launcher.write_text(FAKE_LAUNCHER.replace("#!PYTHON", "#!" + sys.executable))
            launcher.chmod(0o755)
            materialized = root / "materialized.json"
            expected = {"kind": "Render", "diagnostics": [], "fallbacks": [], "metrics": [],
                        "hashes": [], "probes": [
                            {"name": "initial_load_commit", "value": {"kind": "Bool", "value": True}},
                            {"name": "cancel_idempotent", "value": {"kind": "Bool", "value": True}},
                            {"name": "worker_gpu_commit", "value": {"kind": "Bool", "value": False}},
                            {"name": "state_machine", "value": {"kind": "Text", "value": "valid"}},
                        ], "graph": None}
            materialized.write_text(json.dumps([{
                "id": "t8_asset_store__default", "scene": "scenes/fake.ron", "launcher_args": [],
                "render": {"frames": 1, "width": 32, "height": 32},
                "time": {"mode": "seek", "simulation_time": 0.0, "fixed_dt": 0.0166667},
                "expected_result": expected,
            }]))
            report = root / "reports" / "run-1"
            command = [sys.executable, "-c", "pass"]
            evidence = {
                "kind": "Render", "diagnostics": [], "fallbacks": [], "metrics": {}, "hashes": {},
                "probes": {"initial_load_commit": True, "cancel_idempotent": True,
                           "worker_gpu_commit": False, "state_machine": "valid"},
                "asset_events": [{"event": "loaded", "generation": 1}], "graph": None,
            }
            evidence_command = [sys.executable, "-c", f"import json; print({json.dumps(json.dumps(evidence))!r})"]
            result = subprocess.run([
                sys.executable, str(RUNNER), "--slice", "T8.1", "--materialized", str(materialized),
                "--launcher", str(launcher), "--report-dir", str(report), "--case-timeout", "3",
                "--grace-period", ".1", "--command-json", json.dumps(command),
                "--evidence-command-json", json.dumps(evidence_command),
            ], cwd=ROOT, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            aggregate = json.loads((report / "aggregate.json").read_text())
            self.assertTrue(aggregate["command_pass"])
            self.assertTrue(aggregate["passed"])
            item = aggregate["variants"][0]
            self.assertTrue(item["passed"], item["errors"])
            events = json.loads((report / item["id"] / "asset-events.json").read_text())
            self.assertEqual(events[0]["generation"], 1)
            for artifact in ("stdout", "stderr", "launcher.log", "output.png", "report.html"):
                self.assertTrue((report / item["id"] / artifact).exists(), artifact)
            self.assertTrue((report / "report.html").exists())


if __name__ == "__main__":
    unittest.main()
