#!/usr/bin/env python3
"""Fingerprint the exact tracked and untracked source state used for a run."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=root)


def fingerprint(root: Path) -> dict[str, str | bool]:
    commit = git(root, "rev-parse", "HEAD").decode().strip()
    entries = git(root, "status", "--porcelain=v1", "-z", "--untracked-files=all").split(b"\0")
    changed = [entry for entry in entries if entry]
    hasher = hashlib.sha256()
    dirty = bool(changed)
    hasher.update(b"aether-working-tree-v1\0")
    hasher.update(commit.encode())
    hasher.update(b"\0")
    if not dirty:
        tree = git(root, "rev-parse", "HEAD^{tree}").decode().strip()
        hasher.update(b"clean\0")
        hasher.update(tree.encode())
    else:
        index = 0
        while index < len(changed):
            entry = changed[index]
            index += 1
            status = entry[:2]
            path_bytes = entry[3:]
            hasher.update(status)
            hasher.update(b"\0")
            path = os.fsdecode(path_bytes)
            hasher.update(path_bytes)
            hasher.update(b"\0")
            source = root / path
            if source.is_symlink():
                hasher.update(b"symlink\0")
                hasher.update(os.fsencode(os.readlink(source)))
            elif source.is_file():
                hasher.update(b"file\0")
                with source.open("rb") as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        hasher.update(chunk)
            else:
                hasher.update(b"absent\0")
            if status[:1] in (b"R", b"C") and index < len(changed):
                renamed_path = changed[index]
                index += 1
                hasher.update(b"rename\0")
                hasher.update(renamed_path)
    return {"commit": commit, "dirty": dirty, "fingerprint": hasher.hexdigest()}


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    print(json.dumps(fingerprint(root), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
