#!/usr/bin/env python3
"""Run real stdio MCP -> live editor checks on an empty disposable document.

Creates a frame and two rectangles, then undoes all five committed changes unless
--keep-fixture is passed. Never target a working design. Unknown provider token
usage is omitted from metrics.
"""
import argparse
import base64
import json
import pathlib
import selectors
import subprocess
import time


class Client:
    def __init__(self, binary, workspace):
        self.process = subprocess.Popen(
            [binary, "--workspace", str(workspace.resolve())],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        self.ready = selectors.DefaultSelector()
        self.ready.register(self.process.stdout, selectors.EVENT_READ)
        self.request_id = 0
        self.tool_calls = 0

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def rpc(self, method, params):
        self.request_id += 1
        self.send({"jsonrpc": "2.0", "id": self.request_id, "method": method, "params": params})
        while True:
            if not self.ready.select(timeout=25):
                raise TimeoutError("MCP response timed out")
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("MCP subprocess closed")
            response = json.loads(line)
            if response.get("id") == self.request_id:
                if "error" in response:
                    raise RuntimeError(response["error"])
                return response["result"]

    def call(self, name, args):
        self.tool_calls += 1
        result = self.rpc("tools/call", {"name": name, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(result)
        value = result.get("structuredContent") or result
        if "content" in result and "content" not in value:
            value = dict(value)
            value["content"] = result["content"]
        return value

    def close(self):
        self.ready.close()
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def run(options):
    options.output.mkdir(parents=True, exist_ok=True)
    client = Client(options.mcp, options.output)
    started = time.monotonic()
    succeeded = False
    try:
        client.rpc("initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "live-smoke", "version": "1"},
        })
        client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        endpoint = str(options.endpoint.resolve())
        doc = client.call("desigma_attach_editor", {"endpoint_path": endpoint})
        if doc["name"] != options.expected_document_name:
            raise RuntimeError("Target document name changed")
        if doc["node_count"] != len(doc["pages"]):
            raise RuntimeError("Smoke test requires an empty disposable document")
        revision = doc["revision"]
        page = doc["pages"][0]["node_id"]

        def create(parent, kind, width, height):
            nonlocal revision
            result = client.call("desigma_create_node", {
                "expected_revision": revision, "parent_id": parent, "kind": kind,
                "width": width, "height": height, "name": "Agent smoke " + kind,
            })
            revision = result["revision"]
            return result["node"]["id"]

        frame = create(page, "frame", 200, 80)
        create(frame, "rectangle", 20, 20)
        child = create(frame, "rectangle", 20, 20)
        layout = {
            "direction": "Horizontal", "wrap": False, "item_spacing": 10,
            "counter_spacing": 0, "padding": [0, 0, 0, 0],
            "primary_align": "SpaceBetween", "counter_align": "Start",
            "primary_sizing": "Fixed", "counter_sizing": "Fixed",
            "stroke_included": False, "reverse_z": False,
        }
        result = client.call("desigma_set_auto_layout", {
            "expected_revision": revision, "id": frame, "auto_layout": layout,
        })
        revision = result["revision"]
        before = client.call("desigma_get_node", {"node_id": child})["node"]
        assert before["transform"]["tx"] == 180, before
        args = {
            "expected_revision": revision,
            "ops": [{"op": "set_properties", "id": frame, "properties": {"width": 300}}],
        }
        preview = client.call("desigma_apply_ops", dict(args, dry_run=True))
        assert preview["dry_run"] and preview["revision"] == revision, preview
        result = client.call("desigma_apply_ops", args)
        revision = result["revision"]
        after = client.call("desigma_get_node", {"node_id": child})["node"]
        assert after["transform"]["tx"] == 280, after
        computed = client.call("desigma_get_layout", {"node_id": frame})
        assert computed["revision"] == revision, computed
        client.call("desigma_get_diagnostics", {"node_id": frame})
        rendered = client.call("desigma_render_node", {"node_id": frame})
        data = next(content["data"] for content in rendered["content"] if content["type"] == "image")
        render_path = options.output / "render.png"
        render_path.write_bytes(base64.b64decode(data))
        context = client.call("desigma_get_live_context", {})
        assert context["revision"] == revision, context
        if not options.keep_fixture:
            for _ in range(5):
                undone = client.call("desigma_undo", {"expected_revision": revision})
                revision = undone["revision"]
            assert client.call("desigma_get_document", {})["node_count"] == len(doc["pages"])
        state = {
            "endpoint_path": endpoint, "frame": frame, "child": child,
            "revision": revision, "fixture_retained": options.keep_fixture,
        }
        (options.output / "state.json").write_text(json.dumps(state), encoding="utf-8")
        succeeded = True
        print(json.dumps(dict(state, live_roundtrip="passed", tool_calls=client.tool_calls, render=str(render_path))))
    finally:
        client.close()
        metrics = {
            "task": "live_resize_preview_render", "model": "deterministic-smoke",
            "success": succeeded, "tool_calls": client.tool_calls,
            "duration_ms": int((time.monotonic() - started) * 1000),
        }
        with (options.output / "metrics.jsonl").open("a", encoding="utf-8") as output:
            output.write(json.dumps(metrics) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--mcp", default="target/debug/desigma-mcp")
    parser.add_argument("--expected-document-name", required=True)
    parser.add_argument("--keep-fixture", action="store_true")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
