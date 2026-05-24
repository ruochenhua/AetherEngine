#!/usr/bin/env python3
"""
AetherEngine Blog Auto-Generator

Triggered when an OpenSpec change is archived.
Reads the archive contents and generates a Hexo blog post draft.
"""

import os
import sys
import re
import glob
from datetime import datetime
from pathlib import Path

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

AETHER_ROOT = Path("E:/Projects/AetherEngine")
BLOG_ROOT = Path("E:/Projects/ruochenhua.github.io/blog_source")
POSTS_DIR = BLOG_ROOT / "source" / "_posts"
DRAFTS_DIR = BLOG_ROOT / "source" / "_drafts"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def find_latest_archive():
    """Find the most recent OpenSpec archive directory."""
    archive_dir = AETHER_ROOT / "openspec" / "changes" / "archive"
    if not archive_dir.exists():
        return None
    dirs = [d for d in archive_dir.iterdir() if d.is_dir()]
    if not dirs:
        return None
    # Sort by name (YYYY-MM-DD-...)
    dirs.sort(key=lambda d: d.name)
    return dirs[-1]

def read_archive_docs(archive_path):
    """Read proposal, design, tasks from archive."""
    docs = {}
    for name in ["proposal.md", "design.md", "tasks.md"]:
        p = archive_path / name
        if p.exists():
            docs[name] = p.read_text(encoding="utf-8")
    # Also read specs
    specs = []
    specs_dir = archive_path / "specs"
    if specs_dir.exists():
        for spec_dir in sorted(specs_dir.iterdir()):
            if spec_dir.is_dir():
                spec_md = spec_dir / "spec.md"
                if spec_md.exists():
                    specs.append(spec_md.read_text(encoding="utf-8"))
    docs["specs"] = specs
    return docs

def extract_title(proposal_text):
    """Extract title from proposal. Use archive name or first meaningful heading."""
    # Try ## What Changes section - use that as title
    m = re.search(r'^##\s*What Changes\s*$', proposal_text, re.MULTILINE | re.IGNORECASE)
    if m:
        # Look for first bullet point after What Changes
        after = proposal_text[m.end():]
        bullet = re.search(r'^-\s+(.+)$', after, re.MULTILINE)
        if bullet:
            line = bullet.group(1).strip()
            # Extract action noun, e.g. "创建 examples/01_triangle" -> "最小可运行示例"
            if 'examples' in line or 'triangle' in line.lower():
                return "最小可运行示例与引擎启动"
            if 'egui' in line.lower():
                return "egui 调试面板集成"
            if 'scene' in line.lower() or 'gltf' in line.lower():
                return "场景加载系统"
            if 'defer' in line.lower() or 'gbuffer' in line.lower():
                return "延迟渲染管线"
            if 'shadow' in line.lower():
                return "阴影系统"
            if 'ibl' in line.lower() or 'skybox' in line.lower():
                return "IBL 环境光照"
            return line[:30]
    
    # Try ## Why section - skip it, look for next heading
    lines = proposal_text.splitlines()
    for i, line in enumerate(lines):
        if line.strip().startswith('## ') and not line.strip().lower().startswith('## why'):
            title = line.strip()[3:].strip().strip('"\'')
            if title.lower() not in ('why', 'what changes', 'capabilities', 'impact'):
                return title
    
    # Try frontmatter
    m = re.search(r'^title:\s*(.+)$', proposal_text, re.MULTILINE)
    if m:
        return m.group(1).strip().strip('"\'')
    
    return "AetherEngine 开发记录"

def extract_summary(proposal_text, design_text=""):
    """Extract a 2-3 sentence summary from Why/What sections."""
    # Look for ## Why or ## What Changes section
    text = proposal_text
    why_match = re.search(r'##\s*Why\s*\n+(.+?)(?=\n##|\Z)', text, re.DOTALL)
    what_match = re.search(r'##\s*What Changes\s*\n+(.+?)(?=\n##|\Z)', text, re.DOTALL)
    
    summary_parts = []
    if why_match:
        summary = why_match.group(1).strip().replace('\n', ' ')
        summary_parts.append(summary)
    if what_match:
        # Take first bullet or first line
        what_text = what_match.group(1).strip()
        first_line = what_text.split('\n')[0].strip().lstrip('-').strip()
        if first_line:
            summary_parts.append(first_line)
    
    if summary_parts:
        full = " ".join(summary_parts)
        if len(full) > 200:
            full = full[:197] + "..."
        return full
    
    # Fallback: first paragraph
    lines = text.splitlines()
    in_frontmatter = False
    paragraphs = []
    current = []
    for line in lines:
        if line.strip() == "---":
            in_frontmatter = not in_frontmatter
            continue
        if in_frontmatter:
            continue
        if line.strip():
            current.append(line.strip())
        else:
            if current:
                paragraphs.append(" ".join(current))
                current = []
    if current:
        paragraphs.append(" ".join(current))
    
    if paragraphs:
        summary = paragraphs[0]
        if len(summary) > 150:
            summary = summary[:147] + "..."
        return summary
    return "AetherEngine 新功能开发记录。"

def slugify(title):
    """Convert title to URL-friendly slug."""
    slug = title.lower()
    slug = re.sub(r'[^\w\s-]', '', slug)
    slug = re.sub(r'[-\s]+', '-', slug)
    return slug.strip('-')

def generate_post(archive_path, docs):
    """Generate a Hexo blog post from archive docs."""
    archive_name = archive_path.name
    proposal = docs.get("proposal.md", "")
    design = docs.get("design.md", "")
    tasks = docs.get("tasks.md", "")
    specs = docs.get("specs", [])
    
    title = extract_title(proposal)
    summary = extract_summary(proposal, design)
    slug = slugify(title)
    date_str = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    date_path = datetime.now().strftime("%Y/%m/%d")
    
    # Build spec summaries - use directory name as spec title
    spec_summaries = []
    specs_dir = archive_path / "specs"
    if specs_dir.exists():
        spec_dirs = sorted([d for d in specs_dir.iterdir() if d.is_dir()])
        for i, spec_dir in enumerate(spec_dirs[:5], 1):
            # Use directory name as title (e.g. "window-bootstrap" -> "Window Bootstrap")
            spec_name = spec_dir.name.replace('-', ' ').replace('_', ' ')
            spec_name = spec_name.title()
            # Skip generic names
            if spec_name.lower() not in ('added requirements', 'modified requirements', 'requirements'):
                spec_summaries.append(f"{i}. **{spec_name}**")
    
    spec_section = "\n".join(spec_summaries) if spec_summaries else "- 核心模块实现"
    
    # Build design summary from decisions
    design_summary = ""
    if design:
        decisions = re.findall(r'###\s+Decision \d+.*?\n\*\*选择\*\*:\s*(.+?)(?=\n\*\*rationale\*\*:|\n###|\Z)', design, re.DOTALL)
        if not decisions:
            decisions = re.findall(r'###\s+Decision \d+.*?\n\*\*选择\*\*:\s*(.+?)(?=\n\*\*rationale\*\*:|\n###|\Z)', design, re.DOTALL | re.IGNORECASE)
        if decisions:
            design_summary = "\n".join([f"- {d.strip()}" for d in decisions[:3]])
    
    if not design_summary:
        design_summary = "- 详见设计文档"
    
    post = f"""---
title: {title}
date: {date_str}
categories:
  - 技术漫谈
tags: [3D, render, Rust, wgpu, 编程]
index_img: /{date_path}/{slug}/banner.png
banner_img: /{date_path}/{slug}/banner.png
---

{summary}

## 前言

{summary}

本文记录 AetherEngine 中 **{title}** 的设计与实现过程。

## 原理

{design_summary}

## 实现

{spec_section}

## 核心代码

```rust
// TODO: 从实现中提取关键代码片段
```

## 效果

<!-- 截图或录屏 -->

## 总结

- 实现过程中的关键决策
- 遇到的挑战与解决方案
- 后续优化方向

---

*本文对应 AetherEngine 变更：`{archive_name}`*
"""
    return post, slug

def main():
    archive = find_latest_archive()
    if not archive:
        print("No OpenSpec archive found.")
        sys.exit(1)
    
    print(f"Found archive: {archive.name}")
    docs = read_archive_docs(archive)
    post_content, slug = generate_post(archive, docs)
    
    # Write to drafts first (user will review before publishing)
    DRAFTS_DIR.mkdir(parents=True, exist_ok=True)
    draft_path = DRAFTS_DIR / f"{slug}.md"
    draft_path.write_text(post_content, encoding="utf-8")
    
    print(f"Draft generated: {draft_path}")
    print(f"\nTitle: {extract_title(docs.get('proposal.md', ''))}")
    print(f"Slug:  {slug}")
    print(f"\nNext steps:")
    print(f"  1. Review the draft at: {draft_path}")
    print(f"  2. Move to {POSTS_DIR} to publish")
    print(f"  3. Run: cd {BLOG_ROOT} && hexo generate && hexo deploy")

if __name__ == "__main__":
    main()
