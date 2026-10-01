#!/usr/bin/env python3
"""Capture a T8 asset slice scene with bounded launcher lifetime."""

import argparse
import os
from pathlib import Path
import subprocess
import sys


WORKSPACE = Path(__file__).resolve().parents[1]
TIMEOUT_SECONDS = 120


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case-dir", type=Path, required=True)
    parser.add_argument("--scene", default="scenes/t8_material_reload.ron")
    args = parser.parse_args()
    case_dir = args.case_dir.resolve()
    case_dir.mkdir(parents=True, exist_ok=True)
    output = case_dir / "launcher-output.png"
    command = [
        "cargo", "run", "--release", "-p", "aether-launcher", "--",
        "--scene", args.scene,
        "--screenshot", str(output),
        "--exit-after-frames", "1",
        "--no-gui-overlay",
        "--no-debug-helpers",
        "--width", "640",
        "--height", "360",
        "--time-mode", "seek",
        "--simulation-time", "0",
    ]
    with (case_dir / "launcher.stdout").open("wb") as stdout, (case_dir / "launcher.stderr").open("wb") as stderr:
        options = {"cwd": WORKSPACE, "stdout": stdout, "stderr": stderr}
        if os.name == "nt":
            options["creationflags"] = subprocess.CREATE_NO_WINDOW
        process = subprocess.Popen(command, **options)
        try:
            exit_code = process.wait(timeout=TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            print(f"launcher exceeded {TIMEOUT_SECONDS}s and was terminated", file=sys.stderr)
            return 124
    if exit_code != 0:
        print(f"launcher exited with {exit_code}; inspect launcher.stdout and launcher.stderr", file=sys.stderr)
        return exit_code
    if not output.is_file() or output.stat().st_size <= 64:
        print("launcher exited without producing launcher-output.png", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
