"""Contract tests for VisualCase v2 expectation evaluation."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("visual_case_runner", ROOT / "scripts/run_visual_case_v2.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class VisualCaseRunnerTests(unittest.TestCase):
    def test_typed_render_expectations_are_checked_against_observed_evidence(self):
        expected = {
            "kind": "Render", "diagnostics": [{"code": "Loaded", "severity": "info"}],
            "fallbacks": [{"scope": "primitive", "mode": "static"}],
            "metrics": [{"name": "joint_count", "operator": "Eq", "value": 129}],
            "hashes": [], "probes": [
                {"name": "fallback", "value": {"kind": "Bool", "value": True}},
                {"name": "asset_events", "value": {"kind": "Present"}},
            ], "graph": None,
        }
        actual = {
            "kind": "Render", "diagnostics": [{"code": "Loaded", "severity": "info"}],
            "fallbacks": [{"scope": "primitive", "mode": "static"}],
            "metrics": {"joint_count": 129}, "hashes": {},
            "probes": {"fallback": True, "asset_events": ["loaded"]}, "graph": None,
        }
        self.assertEqual(runner.compare(expected, actual), [])
        actual["probes"]["fallback"] = False
        self.assertTrue(any("probe fallback" in error for error in runner.compare(expected, actual)))

    def test_expected_error_is_classified_not_treated_as_arbitrary_process_failure(self):
        expected = {"kind": "ExpectedError", "code": "Cycle", "diagnostics": []}
        self.assertEqual(runner.compare(expected, {"kind": "ExpectedError", "code": "Cycle"}), [])
        self.assertTrue(runner.compare(expected, {"kind": "Render"}))

    def test_launcher_arguments_materialize_render_and_seek_configuration(self):
        case = {"render": {"frames": 1, "width": 64, "height": 32},
                "time": {"mode": "seek", "simulation_time": 0.25, "fixed_dt": 1 / 60},
                "launcher_args": ["--fixture-variant", "valid_128"]}
        args = runner.launcher_args(case, Path("out.png"))
        self.assertIn("--simulation-time", args)
        self.assertIn("0.25", args)
        self.assertEqual(args[-2:], ["--fixture-variant", "valid_128"])

    def test_t7_scene_capture_requires_a_drawable_skinned_item_at_the_seek_time(self):
        case = {"time": {"simulation_time": 0.5}}
        log = "T7_ANIMATION_FRAME frame_id=4 sample_time=0.500000000 skinned_items=1 drawable_items=1\n"
        observed, errors = runner.check_t7_animation_capture("T7.2", case, log)
        self.assertEqual(errors, [])
        self.assertEqual(observed["skinned_items"], 1)
        observed, errors = runner.check_t7_animation_capture(
            "T7.3", {"time": {"simulation_time": 0.25}}, log
        )
        self.assertTrue(any("seek time" in error for error in errors))
        self.assertIsNotNone(observed)

    def test_t7_scene_capture_fails_without_drawable_animation_evidence(self):
        observed, errors = runner.check_t7_animation_capture(
            "T7.2", {"time": {"simulation_time": 0.5}}, ""
        )
        self.assertIsNone(observed)
        self.assertTrue(any("T7_ANIMATION_FRAME" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
