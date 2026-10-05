#!/usr/bin/env python3
"""Capture the bounded glTF skinning and animation review fixture."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys


WORKSPACE = Path(__file__).resolve().parents[1]
TIMEOUT_SECONDS = 120
SCENE = "scenes/t7_skinned_shadow_reflection.ron"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-dir", type=Path)
    args = parser.parse_args()
    run_id = datetime.now(timezone.utc).strftime("t7-visual-%Y%m%dT%H%M%SZ")
    report_dir = (args.report_dir or WORKSPACE / "tests" / "reports" / run_id / "t7_skinned_shadow_reflection").resolve()
    report_dir.mkdir(parents=True, exist_ok=True)

    executable = WORKSPACE / "target" / "release" / ("aether-launcher.exe" if os.name == "nt" else "aether-launcher")
    if not executable.is_file():
        print(f"release launcher not found: {executable}", file=sys.stderr)
        return 2

    output = report_dir / "output.png"
    command = [
        str(executable),
        "--scene", SCENE,
        "--animation", "on",
        "--screenshot", str(output),
        "--exit-after-frames", "1",
        "--no-gui-overlay",
        "--no-debug-helpers",
        "--width", "960",
        "--height", "540",
        "--time-mode", "seek",
        "--simulation-time", "0",
    ]
    stdout_path = report_dir / "launcher.stdout"
    stderr_path = report_dir / "launcher.stderr"
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
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

    log = "\n".join(
        path.read_text(encoding="utf-8", errors="replace")
        for path in (stdout_path, stderr_path)
    )
    (report_dir / "launcher.log").write_text(log, encoding="utf-8")
    (report_dir / "command.txt").write_text(" ".join(command), encoding="utf-8")
    if exit_code != 0:
        print(f"launcher exited with {exit_code}; inspect launcher.log", file=sys.stderr)
        return exit_code
    if not output.is_file() or output.stat().st_size <= 64:
        print("launcher exited without producing output.png", file=sys.stderr)
        return 1
    if "panicked at" in log or "Validation Error" in log:
        print("launcher reported a panic or wgpu validation error", file=sys.stderr)
        return 1

    hash_line = next((line for line in log.splitlines() if line.startswith("T7_POSE_HASHES=")), None)
    pose_hashes = hash_line.partition("=")[2].split(",") if hash_line else []
    if len(pose_hashes) != 4 or any(
        len(value) != 64 or any(char not in "0123456789abcdef" for char in value)
        for value in pose_hashes
    ):
        print("launcher did not report all four deterministic pose hashes", file=sys.stderr)
        return 1

    aggregate = {
        "slice": "T7.3",
        "status": "VISUAL_REVIEW_READY",
        "scene": SCENE,
        "source_asset": "tests/fixtures/t7_animated_actor.gltf",
        "animation_clip": "RaiseArms",
        "animation_frame_extraction": True,
        "pose_samples": [
            {"time": time, "sha256": digest}
            for time, digest in zip((0.0, 0.25, 0.5, 0.75), pose_hashes)
        ],
        "consumers": ["GBuffer", "Shadow", "WaterReflection"],
        "launcher_exit_code": exit_code,
        "exit_after_frames": 1,
        "window_on_screen": False,
        "screenshot": "output.png",
        "screenshot_bytes": output.stat().st_size,
        "limitations": ["Dedicated glTF acceptance actor; generic scene-config spawning and editor animation controls remain separate work."],
    }
    (report_dir / "aggregate.json").write_text(json.dumps(aggregate, indent=2), encoding="utf-8")
    hash_rows = "".join(
        f"<tr><td>{sample['time']}</td><td><code>{sample['sha256']}</code></td></tr>"
        for sample in aggregate["pose_samples"]
    )
    report = f"""<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>T7 glTF 蒙皮与动画画面验收</title><style>body{{font:15px/1.55 system-ui,sans-serif;max-width:1120px;margin:2rem auto;padding:0 1rem;color:#172033;background:#f8fafc}}h1{{margin-bottom:.25rem}}.status{{padding:.8rem 1rem;border-left:4px solid #16803c;background:#effaf2}}.capture{{display:block;width:100%;height:auto;background:#111;border:1px solid #cbd5e1}}code{{overflow-wrap:anywhere}}li{{margin:.3rem 0}}table{{border-collapse:collapse;width:100%}}td,th{{text-align:left;border-bottom:1px solid #ccd;padding:.4rem}}</style></head><body><h1>T7 glTF 蒙皮与动画视觉验收</h1><p class=\"status\">可供人工验收：加载 glTF actor 并渲染完成；测试窗口移出可视区域，进程已退出。</p><p>左侧青色角色显示绑定姿势，右侧橙色角色由 <code>RaiseArms</code> 动画通道驱动到末帧。角色经 owned glTF document、骨架/clip、AnimatorState 和 AnimationFrame 生成 GPU palette；水面倒影同步显示变形结果。测试场景只渲染 1 帧后自动退出。</p><img class=\"capture\" src=\"output.png\" alt=\"T7 glTF 蒙皮、动画、阴影和水面反射测试画面\"><h2>四个确定性姿势</h2><p>SHA-256 由 glTF clip 在固定采样时刻计算；引擎单测锁定四个值，运行日志与结构化报告保留本次结果。</p><table><thead><tr><th>时间（秒）</th><th>pose_state SHA-256</th></tr></thead><tbody>{hash_rows}</tbody></table><h2>验收范围</h2><ul><li>glTF 文件解析出角色网格、三关节 skin 和两条 Linear rotation channels。</li><li>AnimatorState 支持 seek、play、pause、speed、loop 与反向播放。</li><li>AnimationFrame 将采样姿势转换为 GBuffer、Shadow、WaterReflection 共用的 GPU palette。</li></ul><p>限制：此为专用 glTF 验收角色；通用场景配置生成与编辑器动画控件仍待后续接入。</p><p><a href=\"aggregate.json\">aggregate.json</a> · <a href=\"launcher.log\">launcher.log</a> · <a href=\"command.txt\">运行参数</a></p></body></html>"""
    (report_dir / "report.html").write_text(report, encoding="utf-8")
    print(report_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
