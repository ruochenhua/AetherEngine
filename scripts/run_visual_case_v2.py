#!/usr/bin/env python3
"""Execute materialized VisualCase v2 variants and evaluate their runtime evidence."""
import argparse
from datetime import datetime, timezone
import html
import json
import operator
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts/runner_process.py"
OPS = {"Eq": operator.eq, "Ne": operator.ne, "Lt": operator.lt,
       "Le": operator.le, "Gt": operator.gt, "Ge": operator.ge}


def launcher_args(case, output):
    render, time_cfg = case["render"], case["time"]
    args = ["--screenshot", str(output), "--exit-after-frames", str(render["frames"]),
            "--width", str(render["width"]), "--height", str(render["height"]),
            "--no-gui-overlay"]
    if time_cfg["mode"] == "seek":
        args += ["--time-mode", "seek", "--simulation-time", str(time_cfg["simulation_time"]),
                 "--fixed-dt", str(time_cfg["fixed_dt"])]
    else:
        args += ["--freeze-time"]
    return args + case["launcher_args"]


def load_evidence(directory):
    path = directory / "outcome.json"
    if not path.exists():
        return {"kind": "Render", "diagnostics": [], "fallbacks": [], "metrics": {},
                "hashes": {}, "probes": {}, "graph": None, "evidence_status": "missing"}
    data = json.loads(path.read_text())
    if not isinstance(data, dict):
        raise ValueError(f"{path}: outcome must be a JSON object")
    data.setdefault("evidence_status", "present")
    return data


def compare(expected, actual):
    errors = []
    kind = expected["kind"]
    if actual.get("kind") != kind:
        errors.append(f"kind: expected {kind}, got {actual.get('kind')}")
    if kind == "ExpectedError":
        if actual.get("code") != expected["code"]:
            errors.append(f"code: expected {expected['code']}, got {actual.get('code')}")
    for item in expected.get("diagnostics", []):
        if item not in actual.get("diagnostics", []):
            errors.append(f"missing diagnostic {item}")
    if kind == "Render":
        for item in expected.get("fallbacks", []):
            if item not in actual.get("fallbacks", []):
                errors.append(f"missing fallback {item}")
        for metric in expected.get("metrics", []):
            value = actual.get("metrics", {}).get(metric["name"])
            if value is None or not OPS[metric["operator"]](value, metric["value"]):
                errors.append(f"metric {metric['name']}: expected {metric['operator']} {metric['value']}, got {value}")
        for item in expected.get("hashes", []):
            value = actual.get("hashes", {}).get(item["name"])
            if value != item["sha256"]:
                errors.append(f"hash {item['name']}: expected {item['sha256']}, got {value}")
        for item in expected.get("probes", []):
            value = actual.get("probes", {}).get(item["name"], _MISSING)
            if not _matches_probe(item["value"], value):
                errors.append(f"probe {item['name']}: expected {item['value']}, got {None if value is _MISSING else value}")
        graph = expected.get("graph")
        if graph is not None:
            observed = actual.get("graph") or {}
            for key in ("reads", "writes", "edges"):
                if observed.get(key) != graph[key]:
                    errors.append(f"graph {key}: expected {graph[key]}, got {observed.get(key)}")
    return errors


_MISSING = object()


def _matches_probe(expected, actual):
    if actual is _MISSING:
        return False
    kind, value = expected["kind"], expected.get("value")
    if kind == "Present":
        return actual is not None
    if kind == "F32":
        return isinstance(actual, (int, float)) and abs(actual - value["value"]) <= value["tolerance"]
    if kind == "Rgba8":
        return actual == value
    return actual == value


def write_case_report(directory, expected, actual, errors):
    (directory / "outcome.json").write_text(json.dumps(actual, indent=2, sort_keys=True) + "\n")
    (directory / "metrics.json").write_text(json.dumps(actual.get("metrics", {}), indent=2, sort_keys=True) + "\n")
    graph = actual.get("graph")
    (directory / "graph.txt").write_text(
        json.dumps(graph, indent=2, sort_keys=True) + "\n" if graph is not None else "Graph evidence unavailable.\n"
    )
    events = actual.get("asset_events", [])
    (directory / "asset-events.json").write_text(json.dumps(events, indent=2, sort_keys=True) + "\n")
    if "animation_state" in actual:
        (directory / "animation-state.json").write_text(
            json.dumps(actual["animation_state"], indent=2, sort_keys=True) + "\n")
    if "gltf_diagnostics" in actual:
        (directory / "gltf-diagnostics.json").write_text(
            json.dumps(actual["gltf_diagnostics"], indent=2, sort_keys=True) + "\n")
    elif actual.get("diagnostics") is not None:
        (directory / "gltf-diagnostics.json").write_text(
            json.dumps(actual["diagnostics"], indent=2, sort_keys=True) + "\n")
    required_artifacts = ["report.html", "stdout", "stderr", "launcher.log", "graph.txt",
                          "output.png", "reference.png", "diff.png", "metrics.json", "asset-events.json"]
    passed = not errors
    body = [f"<h1>{'PASS' if passed else 'FAIL'}: {html.escape(directory.name)}</h1>",
            f"<h2>Expected</h2><pre>{html.escape(json.dumps(expected, indent=2))}</pre>",
            f"<h2>Observed</h2><pre>{html.escape(json.dumps(actual, indent=2))}</pre>"]
    if errors:
        body.append("<h2>Failures</h2><ul>" + "".join(f"<li>{html.escape(error)}</li>" for error in errors) + "</ul>")
    body.append("<p>Artifacts: stdout, stderr, launcher.log, outcome.json, asset-events.json; screenshot is produced by the launcher.</p>")
    (directory / "report.html").write_text("<!doctype html><meta charset='utf-8'><title>VisualCase v2</title>" + "".join(body))
    (directory / "artifacts.json").write_text(json.dumps({
        name: {"present": (directory / name).is_file(), "required_by_slice_contract": name != "report.html"}
        for name in required_artifacts
    }, indent=2, sort_keys=True) + "\n")
    return passed


def compare_reference(case, directory):
    reference = case.get("reference")
    if not reference or not reference.get("sha256"):
        return None
    source = ROOT / reference["path"]
    output = directory / "output.png"
    if not source.is_file() or not output.is_file():
        return {"passed": False, "error": "release reference or output image is missing"}
    target = directory / "reference.png"
    shutil.copy2(source, target)
    diff = directory / "diff.png"
    comparator = ROOT / ".claude/skills/aether-visual-test/scripts/compare_images.py"
    proc = subprocess.run([sys.executable, str(comparator), str(source), str(output),
                           "--threshold", str(case["compare"]["ssim_min"]), "--json", "--diff", str(diff)],
                          cwd=ROOT, capture_output=True, text=True)
    if proc.returncode:
        return {"passed": False, "error": proc.stderr.strip() or proc.stdout.strip()}
    try:
        metrics = json.loads(proc.stdout)
    except json.JSONDecodeError as error:
        return {"passed": False, "error": f"invalid comparator output: {error}"}
    (directory / "metrics.json").write_text(json.dumps(metrics, indent=2, sort_keys=True) + "\n")
    limits = case["compare"]
    ssim_ok = metrics.get("ssim") is not None and metrics["ssim"] >= limits["ssim_min"]
    mae_ok = metrics["mae"] / 255.0 <= limits["mae_max"]
    diff_ok = metrics["diff_pct"] / 100.0 <= limits["diff_percent_max"]
    if limits.get("exact_hash"):
        import hashlib
        ssim_ok = hashlib.sha256(source.read_bytes()).digest() == hashlib.sha256(output.read_bytes()).digest()
    return {"passed": ssim_ok and mae_ok and diff_ok, "metrics": metrics,
            "limits": {key: limits[key] for key in ("ssim_min", "mae_max", "diff_percent_max", "exact_hash")}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--slice", required=True)
    parser.add_argument("--materialized", required=True)
    parser.add_argument("--launcher", required=True)
    parser.add_argument("--report-dir", required=True)
    parser.add_argument("--case-timeout", default="120")
    parser.add_argument("--handshake-timeout", default="30")
    parser.add_argument("--grace-period", default="5")
    parser.add_argument("--command-json", help="JSON argv array for the slice command-pass predicate")
    parser.add_argument("--evidence-command-json", help="JSON argv array for measured fixture evidence")
    args = parser.parse_args()
    cases = json.loads(Path(args.materialized).read_text())
    if not cases:
        raise SystemExit("manifest materialized to zero cases")
    report_root = Path(args.report_dir)
    report_root.mkdir(parents=True, exist_ok=False)
    run_id = report_root.name
    command = json.loads(args.command_json) if args.command_json else None
    if command is None:
        command_map = json.loads((ROOT / "tests/slice-commands.json").read_text())
        command = command_map.get(args.slice)
    if not isinstance(command, list) or not command or not all(isinstance(arg, str) for arg in command):
        raise SystemExit(f"no command-pass argv configured for slice {args.slice}")
    command_result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    (report_root / "command.stdout").write_text(command_result.stdout)
    (report_root / "command.stderr").write_text(command_result.stderr)
    (report_root / "command.json").write_text(json.dumps({
        "argv": command, "exit_code": command_result.returncode,
    }, indent=2) + "\n")
    process_cases = []
    metadata = {}
    for materialized in cases:
        case = materialized["case"] if "case" in materialized else materialized
        case_id = case["id"]
        output = report_root / case_id / "output.png"
        output.parent.mkdir(parents=True, exist_ok=False)
        process_cases.append({"id": case_id, "scene": case["scene"],
                              "launcher_args": launcher_args(case, output), "output": str(output)})
        metadata[case_id] = (materialized.get("expected_result"), case, output.parent)
    with tempfile.TemporaryDirectory(prefix="aether-v2-") as temp:
        case_file = Path(temp) / "cases.json"
        proc_report = Path(temp) / "process-reports"
        case_file.write_text(json.dumps(process_cases))
        cmd = [sys.executable, str(RUNNER), "--launcher", args.launcher, "--cases", str(case_file),
               "--report-dir", str(proc_report), "--case-timeout", str(args.case_timeout),
               "--handshake-timeout", str(args.handshake_timeout), "--grace-period", str(args.grace_period)]
        result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        (report_root / "stdout").write_text(result.stdout)
        (report_root / "stderr").write_text(result.stderr)
        results = []
        for case_id, (expected, case, directory) in metadata.items():
            process_log_path = proc_report / case_id / "launcher.log"
            process_log = json.loads(process_log_path.read_text()) if process_log_path.exists() else {"diagnostic": "RunnerDidNotStart"}
            # Preserve process supervisor artifacts beside the per-variant report.
            process_dir = proc_report / case_id
            for name in ("launcher.log", "stdout", "stderr"):
                source = process_dir / name
                if source.exists():
                    (directory / name).write_bytes(source.read_bytes())
            actual = load_evidence(directory)
            if process_log.get("diagnostic"):
                actual["process_diagnostic"] = process_log["diagnostic"]
            errors = compare(expected, actual) if expected is not None else []
            image_result = compare_reference(case, directory)
            if image_result is not None and not image_result["passed"]:
                errors.append(f"reference comparison failed: {image_result}")
            if expected and expected["kind"] == "Render" and not (directory / "output.png").is_file():
                errors.append("rendered output.png is missing")
            if process_log.get("diagnostic"):
                errors.append(f"launcher process: {process_log['diagnostic']}")
            passed = write_case_report(directory, expected, actual, errors)
            results.append({"id": case_id, "passed": passed, "errors": errors,
                            "expected": expected, "observed": actual, "image": image_result,
                            "process": process_log})
    evidence_map = json.loads((ROOT / "tests/slice-evidence-commands.json").read_text())
    evidence_command = (json.loads(args.evidence_command_json)
                        if args.evidence_command_json else evidence_map.get(args.slice))
    if not isinstance(evidence_command, list) or not evidence_command:
        raise SystemExit(f"no evidence argv configured for slice {args.slice}")
    evidence_commands = []
    for case_id, (expected, case, directory) in metadata.items():
        variant_id = case_id.split("__", 1)[1] if "__" in case_id else "default"
        argv = [part.replace("{variant}", variant_id) for part in evidence_command]
        evidence = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
        evidence_commands.append({"id": case_id, "argv": argv, "exit_code": evidence.returncode})
        if evidence.returncode == 0:
            try:
                actual = json.loads(evidence.stdout)
                if not isinstance(actual, dict):
                    raise ValueError("evidence must be a JSON object")
                actual["evidence_status"] = "measured"
                if args.slice == "T7.1" and "gltf_diagnostics" not in actual:
                    actual["gltf_diagnostics"] = actual.get("diagnostics", [])
                if expected is not None:
                    errors = compare(expected, actual)
                    image_result = compare_reference(case, directory)
                    if image_result is not None and not image_result["passed"]:
                        errors.append(f"reference comparison failed: {image_result}")
                    process = next(item for item in results if item["id"] == case_id)
                    if process.get("process", {}).get("diagnostic"):
                        errors.append(f"launcher process: {process['process']['diagnostic']}")
                    process.update({"passed": not errors, "errors": errors, "expected": expected,
                                   "observed": actual, "image": image_result})
                    write_case_report(directory, expected, actual, errors)
            except (ValueError, json.JSONDecodeError) as error:
                evidence_commands[-1]["parse_error"] = str(error)
                next(item for item in results if item["id"] == case_id)["errors"].append(
                    f"evidence output invalid: {error}")
        else:
            evidence_commands[-1]["stderr"] = evidence.stderr[-4000:]
            next(item for item in results if item["id"] == case_id)["errors"].append(
                f"evidence command failed with exit {evidence.returncode}")
    required_slice_artifacts = {
        "T7.1": "gltf-diagnostics.json", "T7.3": "animation-state.json",
        "T8.1": "asset-events.json",
    }
    required_artifact = required_slice_artifacts.get(args.slice)
    if required_artifact:
        for item in results:
            case_dir = report_root / item["id"]
            if not (case_dir / required_artifact).is_file():
                item["errors"].append(f"required artifact {required_artifact} is missing")
                item["passed"] = False
    artifacts = ["report.html", "stdout", "stderr", "launcher.log", "graph.txt", "output.png",
                 "reference.png", "diff.png", "metrics.json", "asset-events.json"]
    if args.slice == "T7.1": artifacts.append("gltf-diagnostics.json")
    if args.slice == "T7.3": artifacts.append("animation-state.json")
    aggregate = {"slice": args.slice, "run_id": run_id,
                 "command_pass": command_result.returncode == 0,
                 "evidence_commands": evidence_commands,
                 "command": command, "command_exit_code": command_result.returncode,
                 "passed": command_result.returncode == 0 and all(item["passed"] for item in results),
                 "variants": results, "artifacts": artifacts}
    (report_root / "aggregate.json").write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    rows = "".join(f"<tr><td>{html.escape(item['id'])}</td><td>{'PASS' if item['passed'] else 'FAIL'}</td><td><a href='{html.escape(item['id'])}/report.html'>details</a></td></tr>" for item in results)
    (report_root / "report.html").write_text(f"<!doctype html><meta charset='utf-8'><title>{html.escape(args.slice)} aggregate</title><h1>{html.escape(args.slice)} {'PASS' if aggregate['passed'] else 'FAIL'}</h1><table><tr><th>Variant</th><th>Status</th><th>Evidence</th></tr>{rows}</table><pre>{html.escape(json.dumps(aggregate, indent=2))}</pre>")
    print(f"VisualCase v2 aggregate: {report_root / 'report.html'}")
    return 0 if aggregate["passed"] and result.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
