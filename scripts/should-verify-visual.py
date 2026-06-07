#!/usr/bin/env python3
"""
智能判定：基于代码变更内容，判断是否需要执行视觉验证。

用法:
    # 基于 git diff（默认对比 HEAD~1）
    python3 scripts/should-verify-visual.py

    # 基于指定 commit range
    python3 scripts/should-verify-visual.py --since HEAD~3

    # 基于显式文件列表
    python3 scripts/should-verify-visual.py --files crates/aether-engine/src/renderer/passes/ssao.rs

    # 结合 issue/PRD 文本做语义增强判定
    python3 scripts/should-verify-visual.py --issue-text "Refactor shadow pass to reduce draw calls"

退出码:
    0 = MUST_VERIFY  (必须视觉验证)
    1 = SHOULD_VERIFY (建议视觉验证)
    2 = NO_VERIFY     (无需视觉验证)
    3 = 判定出错
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


# ── 规则定义 ──────────────────────────────────────────────────────────

# 看到这些文件/目录变更 → MUST_VERIFY
MUST_PATTERNS = [
    r"assets/shaders/.*\.wgsl$",
    r"crates/.*/renderer/passes/.*\.rs$",
    r"crates/.*/renderer/lighting\.rs$",
    r"crates/.*/renderer/scheduler\.rs$",
    r"crates/.*/renderer/context\.rs$",
    r"crates/.*/renderer/frame\.rs$",
    r"crates/.*/renderer/resource\.rs$",
    r"crates/.*/renderer/resource_table\.rs$",
    r"crates/.*/scene/loader\.rs$",
    r"crates/.*/scene/description\.rs$",
    r"crates/.*/asset/shader\.rs$",
    r"crates/.*/asset/material\.rs$",
    r"crates/.*/asset/texture\.rs$",
    r"crates/.*/asset/mesh\.rs$",
    r"scenes/.*\.ron$",
]

# 看到这些文件/目录变更 → SHOULD_VERIFY（非强制，但建议）
SHOULD_PATTERNS = [
    r"crates/.*/renderer/camera\.rs$",
    r"crates/.*/renderer/camera/.*\.rs$",
    r"crates/.*/renderer/ibl\.rs$",
    r"crates/.*/renderer/light\.rs$",
    r"crates/.*/window\.rs$",
    r"crates/.*/input\.rs$",
    r"crates/.*/Cargo\.toml$",
    r"Cargo\.toml$",
]

# 看到这些文件/目录变更 → 大概率 NO_VERIFY（但需结合其他变更综合判断）
NO_VERIFY_PATTERNS = [
    r"^\.github/",
    r"^docs/",
    r"^scripts/(?!should-verify-visual).*",
    r"^\.gitignore$",
    r"^README",
    r"^CLAUDE",
    r"^CONTEXT",
    r"^\.claude/",
    r"^openspec/",
    r"^\.DS_Store",
    r"tests/reports/.*",
    r"tests/output/.*",
    r"tests/reference/.*",
]

# 语义关键词：issue/PRD/commit message 中有这些词 → 提升验证等级
VISUAL_KEYWORDS = [
    "shadow", "ssao", "ssr", "ibl", "lighting", "reflection", "ambient",
    "occlusion", "render", "pass", "shader", "wgsl", "texture", "material",
    "pbr", "brdf", "tone mapping", "post-process", "gamma", "hdr",
    "visual", "screenshot", "画面", "光照", "阴影", "反射", "材质",
]

PERF_KEYWORDS = [
    "perf", "performance", "optimize", "optimization", "speed", "fast",
    "cache", "alloc", "allocator", "parallel", "async", "吞吐量",
]

REFACTOR_KEYWORDS = [
    "refactor", "cleanup", "rename", "extract", "inline", "move",
    "reorganize", "tidy", "整理", "重构",
]


def match_any(path: str, patterns: list[str]) -> bool:
    return any(re.search(p, path) for p in patterns)


def get_changed_files(since: str | None) -> list[str]:
    """通过 git diff 获取变更文件列表。"""
    cmd = ["git", "diff", "--name-only"]
    if since:
        cmd.append(since)
    else:
        cmd.append("HEAD~1")
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        files = [f.strip() for f in result.stdout.strip().split("\n") if f.strip()]
        return files
    except subprocess.CalledProcessError:
        # 可能只有一个 commit 或不是 git repo
        return []


def classify_file(path: str) -> str:
    if match_any(path, MUST_PATTERNS):
        return "MUST"
    if match_any(path, SHOULD_PATTERNS):
        return "SHOULD"
    if match_any(path, NO_VERIFY_PATTERNS):
        return "NO"
    return "UNKNOWN"


def analyze_text(text: str | None) -> dict:
    """分析 issue/PRD/commit message 的语义倾向。"""
    if not text:
        return {"visual_score": 0, "perf_score": 0, "refactor_score": 0}

    text_lower = text.lower()

    visual_score = sum(1 for kw in VISUAL_KEYWORDS if kw.lower() in text_lower)
    perf_score = sum(1 for kw in PERF_KEYWORDS if kw.lower() in text_lower)
    refactor_score = sum(1 for kw in REFACTOR_KEYWORDS if kw.lower() in text_lower)

    return {
        "visual_score": visual_score,
        "perf_score": perf_score,
        "refactor_score": refactor_score,
    }


def decide(
    files: list[str],
    issue_text: str | None = None,
    commit_msg: str | None = None,
) -> tuple[str, dict]:
    """
    综合判定是否需要视觉验证。
    返回: (verdict, details)
    verdict: MUST_VERIFY | SHOULD_VERIFY | NO_VERIFY
    """
    details = {
        "files": {},
        "semantic": {},
        "reason": "",
    }

    # 1. 文件级别判定
    has_must = False
    has_should = False
    has_unknown = False
    all_no = True

    for f in files:
        cls = classify_file(f)
        details["files"][f] = cls
        if cls == "MUST":
            has_must = True
            all_no = False
        elif cls == "SHOULD":
            has_should = True
            all_no = False
        elif cls == "UNKNOWN":
            has_unknown = True
            all_no = False
        elif cls == "NO":
            pass  # all_no 可能保持 True

    # 2. 语义分析
    combined_text = " ".join(filter(None, [issue_text, commit_msg]))
    semantic = analyze_text(combined_text)
    details["semantic"] = semantic

    visual_score = semantic["visual_score"]
    perf_score = semantic["perf_score"]
    refactor_score = semantic["refactor_score"]

    # 3. 综合决策
    if has_must:
        details["reason"] = f"Changed render-critical files: {[f for f, c in details['files'].items() if c == 'MUST']}"
        return "MUST_VERIFY", details

    if visual_score >= 2:
        details["reason"] = f"Issue/PRD text implies visual changes (score={visual_score})"
        return "MUST_VERIFY", details

    if has_should and visual_score >= 1:
        details["reason"] = f"Changed renderer-adjacent files + visual keywords (score={visual_score})"
        return "MUST_VERIFY", details

    if has_should:
        details["reason"] = "Changed renderer-adjacent files but no direct visual evidence"
        return "SHOULD_VERIFY", details

    if all_no and not has_unknown:
        if perf_score >= 2 or refactor_score >= 2:
            details["reason"] = "Pure performance/refactor change with no renderer files touched"
            return "NO_VERIFY", details
        details["reason"] = "No renderer-related files changed"
        return "NO_VERIFY", details

    if has_unknown and visual_score == 0 and (perf_score >= 1 or refactor_score >= 1):
        details["reason"] = "Ambiguous files but text suggests perf/refactor only"
        return "NO_VERIFY", details

    details["reason"] = "Ambiguous change — default to safe side"
    return "SHOULD_VERIFY", details


def main():
    parser = argparse.ArgumentParser(description="Intelligently decide if visual verification is needed")
    parser.add_argument("--since", type=str, help="Git commit range, e.g. HEAD~3")
    parser.add_argument("--files", nargs="+", help="Explicit file list instead of git diff")
    parser.add_argument("--issue-text", type=str, help="Issue/PRD description for semantic analysis")
    parser.add_argument("--commit-msg", type=str, help="Commit message for semantic analysis")
    parser.add_argument("--json", action="store_true", help="Output JSON only")
    args = parser.parse_args()

    if args.files:
        files = args.files
    else:
        files = get_changed_files(args.since)
        if not files:
            print("No changed files detected (maybe first commit?)", file=sys.stderr)
            sys.exit(3)

    verdict, details = decide(files, args.issue_text, args.commit_msg)

    exit_codes = {
        "MUST_VERIFY": 0,
        "SHOULD_VERIFY": 1,
        "NO_VERIFY": 2,
    }

    if args.json:
        print(json.dumps({
            "verdict": verdict,
            "exit_code": exit_codes.get(verdict, 3),
            "details": details,
        }, indent=2))
    else:
        print(f"Verdict: {verdict}")
        print(f"Reason:  {details['reason']}")
        print("")
        print("File classification:")
        for f, cls in details["files"].items():
            print(f"  [{cls:8}] {f}")
        if details["semantic"]:
            print("")
            print("Semantic scores:")
            for k, v in details["semantic"].items():
                print(f"  {k}: {v}")

    sys.exit(exit_codes.get(verdict, 3))


if __name__ == "__main__":
    main()
