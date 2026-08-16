#!/usr/bin/env python3
"""Enforce Aether Engine module-health rules.

Rules:
1. New .rs files under crates/ must not exceed 500 lines.
2. Existing files may temporarily exceed 500 lines, but they must never grow
   beyond their recorded baseline. The only way to add code to an over-limit
   file is to first split/refactor it.

Usage:
  python3 scripts/verify_module_health.py --check
  python3 scripts/verify_module_health.py --update-baseline
"""

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
DEFAULT_BASELINE = ROOT / "scripts" / "module-health-baseline.json"
MAX_LINES = 500


def iter_rs_files():
    for path in sorted(CRATES.rglob("*.rs")):
        yield path


def line_count(path: Path) -> int:
    with path.open("r", encoding="utf-8") as f:
        return sum(1 for _ in f)


def load_baseline(path: Path):
    if not path.exists():
        return {}
    with path.open("r", encoding="utf-8") as f:
        data = json.load(f)
    return data.get("files", {})


def write_baseline(path: Path, files):
    data = {
        "version": 1,
        "max_lines": MAX_LINES,
        "files": files,
    }
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def update_baseline(path: Path):
    files = {}
    for rs in iter_rs_files():
        rel = rs.relative_to(ROOT).as_posix()
        files[rel] = line_count(rs)
    write_baseline(path, files)
    over = {k: v for k, v in files.items() if v > MAX_LINES}
    print(f"Updated baseline: {len(files)} files")
    print(f"Over-limit files: {len(over)}")
    for k, v in sorted(over.items(), key=lambda item: item[1], reverse=True):
        print(f"  {v:5d} {k}")


def check(path: Path):
    baseline = load_baseline(path)
    errors = []
    over_limit = []

    for rs in iter_rs_files():
        rel = rs.relative_to(ROOT).as_posix()
        lines = line_count(rs)
        if rel not in baseline:
            if lines > MAX_LINES:
                errors.append(
                    f"{rel}: new file exceeds {MAX_LINES} lines ({lines})"
                )
        else:
            if lines > baseline[rel]:
                errors.append(
                    f"{rel}: grew from {baseline[rel]} to {lines} lines; split/refactor before adding code"
                )
        if lines > MAX_LINES:
            over_limit.append((lines, rel))

    over_limit.sort(reverse=True)
    print(f"Checked {sum(1 for _ in iter_rs_files())} Rust files")
    print(f"Over-limit files: {len(over_limit)}")
    for lines, rel in over_limit:
        print(f"  {lines:5d} {rel}")

    if errors:
        print("\nModule health violations:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("Module health OK")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Aether Engine module health checks")
    parser.add_argument(
        "--baseline",
        type=Path,
        default=DEFAULT_BASELINE,
        help="Path to baseline JSON",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="Check against baseline")
    mode.add_argument(
        "--update-baseline",
        action="store_true",
        help="Regenerate baseline from current file sizes",
    )
    args = parser.parse_args()

    if args.update_baseline:
        update_baseline(args.baseline)
        return 0

    return check(args.baseline)


if __name__ == "__main__":
    sys.exit(main())
