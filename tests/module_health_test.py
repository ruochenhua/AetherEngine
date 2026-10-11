import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify_module_health.py"
SPEC = importlib.util.spec_from_file_location("verify_module_health", SCRIPT)
HEALTH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HEALTH)


class ModuleHealthTest(unittest.TestCase):
    def run_check(self, files, baseline):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            crates = root / "crates"
            for name, lines in files.items():
                path = crates / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("\n" * lines, encoding="utf-8")

            baseline_path = root / "baseline.json"
            baseline_path.write_text(
                json.dumps({"files": baseline}), encoding="utf-8"
            )
            with patch.object(HEALTH, "ROOT", root), patch.object(
                HEALTH, "CRATES", crates
            ):
                with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
                    io.StringIO()
                ):
                    return HEALTH.check(baseline_path)

    def test_existing_under_limit_file_can_grow_until_500_lines(self):
        result = self.run_check(
            {"src/lib.rs": 251}, {"crates/src/lib.rs": 247}
        )
        self.assertEqual(result, 0)
        result = self.run_check(
            {"src/lib.rs": 501}, {"crates/src/lib.rs": 247}
        )
        self.assertEqual(result, 1)

    def test_existing_over_limit_file_must_not_grow(self):
        result = self.run_check(
            {"src/lib.rs": 501}, {"crates/src/lib.rs": 501}
        )
        self.assertEqual(result, 0)
        result = self.run_check(
            {"src/lib.rs": 500}, {"crates/src/lib.rs": 501}
        )
        self.assertEqual(result, 0)
        result = self.run_check(
            {"src/lib.rs": 502}, {"crates/src/lib.rs": 501}
        )
        self.assertEqual(result, 1)

    def test_new_file_must_stay_within_500_lines(self):
        result = self.run_check({"src/new.rs": 501}, {})
        self.assertEqual(result, 1)


if __name__ == "__main__":
    unittest.main()
