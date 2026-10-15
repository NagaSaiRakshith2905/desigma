#!/usr/bin/env python3
"""Run one real Codex CLI attempt against a disposable Desigma MCP fixture.

Codex event JSONL is the source of tool-call and token telemetry. Unknown token
usage is omitted. Scores are computed by reopening the model's saved document
through a fresh MCP process and checking native document structure.
"""
import argparse
import hashlib
import json
import math
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path


class McpClient:
    def __init__(self, binary, workspace):
        self.process = subprocess.Popen(
            [str(binary), "--workspace", str(workspace)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8",
        )
        self.next_id = 0

    def rpc(self, method, params=None):
        self.next_id += 1
        request_id = self.next_id
        self.process.stdin.write(json.dumps({
            "jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {},
        }) + "\n")
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                error = self.process.stderr.read()
                raise RuntimeError(f"MCP server exited during {method}: {error[-2000:]}")
            message = json.loads(line)
            if message.get("id") != request_id:
                continue
            if "error" in message:
                raise RuntimeError(f"MCP {method}: {message['error']}")
            return message.get("result", {})

    def call(self, name, args):
        result = self.rpc("tools/call", {"name": name, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(f"MCP tool {name} failed: {result}")
        value = result.get("structuredContent")
        if value is not None:
            return value
        for content in result.get("content", []):
            if content.get("type") == "text":
                try:
                    return json.loads(content["text"])
                except (KeyError, json.JSONDecodeError):
                    pass
        return result

    def initialize(self):
        self.rpc("initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "agent-model-eval", "version": "1"},
        })
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        self.process.stdin.flush()

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def node_id(summary):
    for key in ("node_id", "id"):
        if isinstance(summary, dict) and type(summary.get(key)) is int:
            return summary[key]
    raise ValueError(f"page summary has no numeric node ID: {summary}")


def create_node(client, revision, parent, kind, name, width, height, text=None):
    args = {
        "expected_revision": revision, "parent_id": parent, "kind": kind,
        "name": name, "width": width, "height": height,
    }
    if text is not None:
        args["text"] = text
    result = client.call("desigma_create_node", args)
    return result["revision"], int(result["node"]["id"])


def json_path(value, dotted):
    current = value
    for part in dotted.split("."):
        if isinstance(current, list):
            current = current[int(part)]
        else:
            current = current[part]
    return current


def structurally_score(result_file, workspace, mcp_binary, task, refs):
    client = McpClient(mcp_binary, workspace)
    try:
        client.initialize()
        client.call("desigma_open_file", {"path": str(result_file.resolve())})
        assertions = []
        for check in task["evaluation"]["checks"]:
            target = refs[check["target"]]
            node = client.call("desigma_get_node", {"node_id": target})["node"]
            actual = json_path(node, check["path"])
            if "target_ref" in check:
                expected = refs[check["target_ref"]]
                passed = actual == expected
            elif "exists" in check:
                expected = check["exists"]
                passed = (actual is not None) == expected
            else:
                expected = check["equals"]
                if isinstance(actual, (int, float)) and isinstance(expected, (int, float)):
                    passed = math.isclose(float(actual), float(expected), rel_tol=0, abs_tol=1e-4)
                else:
                    passed = actual == expected
            assertions.append({"target": check["target"], "path": check["path"], "expected": expected, "actual": actual, "passed": passed})
        return all(item["passed"] for item in assertions), assertions
    finally:
        client.close()


def event_telemetry(lines):
    tool_calls = 0
    started_calls = 0
    turns = []
    model = None
    for line in lines.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict):
            continue
        context = event.get("turn_context") or event.get("context") or {}
        if isinstance(context, dict):
            model = model or context.get("model")
        model = model or event.get("model")
        item = event.get("item", {})
        item_type = item.get("type") if isinstance(item, dict) else None
        event_type = event.get("type", "")
        if item_type in ("mcp_tool_call", "tool_call") and event_type.endswith("started"):
            started_calls += 1
        if item_type in ("mcp_tool_call", "tool_call") and event_type.endswith("completed") and not started_calls:
            tool_calls += 1
        if event_type == "turn.completed":
            tool_calls += started_calls
            started_calls = 0
            usage = event.get("usage")
            if not isinstance(usage, dict):
                info = event.get("info") or {}
                cumulative = info.get("total_token_usage") if isinstance(info, dict) else None
                usage = cumulative.get("last_token_usage") if isinstance(cumulative, dict) else None
            if isinstance(usage, dict):
                row = {}
                for key in ("input_tokens", "output_tokens"):
                    if type(usage.get(key)) is int and usage[key] >= 0:
                        row[key] = usage[key]
                if row:
                    turns.append(row)
    tool_calls += started_calls
    telemetry = {"tool_calls": tool_calls}
    if model:
        telemetry["model"] = model
    if turns:
        telemetry["input_tokens"] = sum(row.get("input_tokens", 0) for row in turns) if any("input_tokens" in r for r in turns) else None
        telemetry["output_tokens"] = sum(row.get("output_tokens", 0) for row in turns) if any("output_tokens" in r for r in turns) else None
        telemetry = {k: v for k, v in telemetry.items() if v is not None}
    return telemetry


def git_context(root):
    try:
        commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, text=True, capture_output=True, check=True).stdout.strip()
        dirty = bool(subprocess.run(["git", "status", "--porcelain"], cwd=root, text=True, capture_output=True, check=True).stdout.strip())
    except (OSError, subprocess.CalledProcessError):
        commit, dirty = "unknown", None
    return commit, dirty


def run_attempt(args):
    catalog = json.loads(args.catalog.read_text(encoding="utf-8"))
    task = next((t for t in catalog["tasks"] if t["id"] == args.task), None)
    if task is None or "evaluation" not in task:
        raise ValueError(f"task {args.task!r} has no executable evaluation profile")
    binary = args.mcp.resolve()
    codex = shutil.which(str(args.codex)) if not Path(args.codex).is_file() else str(Path(args.codex).resolve())
    if not binary.is_file():
        raise ValueError(f"MCP executable not found: {binary}")
    if not codex:
        raise ValueError("Codex CLI executable not found")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    attempt_dir = Path(tempfile.mkdtemp(prefix=f"{args.task}-", dir=args.output_dir))
    workspace = attempt_dir / "workspace"
    workspace.mkdir()
    fixture_path = workspace / "fixture.desigma"
    result_path = workspace / "result.desigma"
    prep = McpClient(binary, workspace)
    try:
        prep.initialize()
        new_doc = prep.call("desigma_new_document", {"name": "Disposable model evaluation"})
        revision = int(new_doc["revision"])
        page = node_id(new_doc["pages"][0])
        refs = {}
        profile = task["evaluation"]["fixture"]
        if profile in ("text", "visual", "component", "prototype", "structure", "variable"):
            kind = "text" if profile == "text" else "frame" if profile in ("component", "prototype") else "rectangle"
            revision, target = create_node(prep, revision, page, kind, "Evaluation target", 280 if kind == "text" else 220, 72 if kind == "text" else 140, "Editable heading" if kind == "text" else None)
            refs["target"] = target
        if profile == "prototype":
            revision, destination = create_node(prep, revision, page, "frame", "Evaluation destination", 220, 140)
            refs["destination"] = destination
        if profile == "structure":
            revision, destination = create_node(prep, revision, page, "frame", "Evaluation destination", 320, 220)
            refs["destination"] = destination
        if profile == "variable":
            created = prep.call("desigma_apply_ops", {
                "expected_revision": revision,
                "ops": [
                    {"op": "create_variable_collection", "temp_id": "$new1", "name": "Evaluation palette"},
                    {"op": "create_variable", "temp_id": "$new2", "collection_id": "$new1", "name": "Accent", "kind": "Color"},
                    {"op": "edit_variable", "id": "$new2", "mode": "$new1_mode", "value": {"Color": {"r": 0.2, "g": 0.4, "b": 0.8, "a": 1.0}}},
                ],
            })
            revision = int(created["revision"])
            refs["variable_id"] = created["temp_variable_ids"]["$new2"]
        if profile == "resize":
            revision, fill_child = create_node(prep, revision, page, "rectangle", "Fill child", 100, 40)
            revision, fixed_child = create_node(prep, revision, page, "rectangle", "Fixed child", 50, 40)
            auto_layout = {
                "direction": "Horizontal", "wrap": False, "item_spacing": 8,
                "counter_spacing": 0, "padding": [16, 16, 16, 16],
                "primary_align": "Start", "counter_align": "Start",
                "primary_sizing": "Fixed", "counter_sizing": "Hug",
                "stroke_included": False, "reverse_z": False,
            }
            wrapped = prep.call("desigma_wrap_in_auto_layout", {
                "temp_id": "$new1", "ids": [fill_child, fixed_child],
                "auto_layout": auto_layout, "name": "Evaluation target",
                "expected_revision": revision,
            })
            revision = int(wrapped["revision"])
            refs["target"] = int(wrapped["temp_ids"]["$new1"])
            refs["fixed_child"] = fixed_child
        fonts = prep.call("desigma_list_fonts", {})
        prep.call("desigma_save_file", {"path": fixture_path.name})
    finally:
        prep.close()

    prompt = (
        "Use the connected Desigma MCP server named desigma. This is a disposable offline evaluation document. "
        f"First open {fixture_path} with desigma_open_file, inspect the document and identify the layer named Evaluation target. "
        f"Task: {task['prompt']} "
        f"After completing the requested edit, save the result to the new path {result_path}. "
        "Use MCP document tools for every read and write. Do not use shell commands, edit files directly, or inspect unrelated workspace files. "
        "Report briefly what you changed."
    )
    command = [
        codex, "exec", "--json", "--ephemeral", "--approve-for-me",
        "-C", str(attempt_dir.resolve()),
    ]
    if args.model:
        command.extend(["--model", args.model])
    command.extend([
        "-c", "mcp_servers.desigma.command=" + json.dumps(str(binary)),
        "-c", "mcp_servers.desigma.args=" + json.dumps(["--workspace", str(workspace)]),
    ])
    started = time.monotonic()
    proc = subprocess.run(command + [prompt], text=True, capture_output=True, timeout=args.timeout)
    duration = int((time.monotonic() - started) * 1000)
    (attempt_dir / "codex-events.jsonl").write_text(proc.stdout, encoding="utf-8")
    (attempt_dir / "codex-stderr.txt").write_text(proc.stderr, encoding="utf-8")
    telemetry = event_telemetry(proc.stdout)
    assertions = []
    score = False
    if proc.returncode == 0 and result_path.is_file():
        score, assertions = structurally_score(result_path, workspace, binary, task, refs)
    commit, dirty = git_context(args.repo)
    font_rows = fonts.get("fonts", fonts.get("faces", fonts.get("items", []))) if isinstance(fonts, dict) else []
    font_info = {
        "count": len(font_rows) if isinstance(font_rows, list) else None,
        "sha256": hashlib.sha256(json.dumps(font_rows, sort_keys=True).encode()).hexdigest(),
    }
    row = {
        "task": args.task,
        "success": bool(proc.returncode == 0 and score),
        "duration_ms": duration,
        "commit": commit,
        "working_tree_dirty": dirty,
        "os": platform.platform(),
        "transport": "stdio",
        "fixture": task["evaluation"]["fixture"],
        "font_inventory": font_info,
        "structural_assertions": assertions,
        "codex_exit_code": proc.returncode,
        **telemetry,
    }
    with args.results.open("a", encoding="utf-8") as output:
        output.write(json.dumps(row, ensure_ascii=False) + "\n")
    print(json.dumps({"metrics": row, "attempt_dir": str(attempt_dir), "saved_result": result_path.is_file()}, ensure_ascii=False))
    return row["success"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=Path("crates/desigma-mcp/tests/editing-workflows.json"))
    parser.add_argument("--task", required=True)
    parser.add_argument("--mcp", type=Path, default=Path("target/debug/desigma-mcp"))
    parser.add_argument("--codex", default="codex")
    parser.add_argument("--model")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output-dir", type=Path, default=Path("target/agent-evals"))
    parser.add_argument("--results", type=Path, default=Path("target/agent-evals/results.jsonl"))
    parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    try:
        args.results.parent.mkdir(parents=True, exist_ok=True)
        raise SystemExit(0 if run_attempt(args) else 1)
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired, KeyError, json.JSONDecodeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
