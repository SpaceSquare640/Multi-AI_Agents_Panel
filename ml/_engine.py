"""JSON-RPC 2.0 bridge exposing installed ML capabilities over localhost
HTTP — the machine-learning counterpart of `skills/_bridge.py`.

Spawned as a **separate** child process from the Skills bridge by the
Rust `ml_engine` module — see Source_Code/src-tauri/src/ml_engine/mod.rs
and the design note in
Multi-AI Agent Panel Document/04 Agents & Orchestration/ML Engine Design.md.
Kept as its own process (not merged into `skills/_bridge.py`) because ML
packages like `sentence-transformers`/PyTorch have much longer startup
time and memory footprint than the plain-stdlib Skills bridge — mixing
them would slow down every Skill call, even ones with nothing to do with
ML.

Structurally identical to `skills/_bridge.py` (same JSON-RPC 2.0 shape,
same localhost + bearer-token isolation, same manifest-discovery
pattern) — only the manifest filename (`capability.json` instead of
`skill.json`) and the directory it scans differ, since capabilities here
are expected to depend on real ML packages that a plain Skill shouldn't
have to carry.
"""

import argparse
import importlib.util
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


def load_capabilities(ml_dir: Path) -> dict:
    """Imports every `<ml_dir>/<name>/capability.json` + its entrypoint
    module, keyed by manifest name (== JSON-RPC method name)."""
    capabilities = {}
    for manifest_path in sorted(ml_dir.glob("*/capability.json")):
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        entrypoint = manifest_path.parent / manifest["entrypoint"]
        spec = importlib.util.spec_from_file_location(manifest["name"], entrypoint)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        capabilities[manifest["name"]] = module
    return capabilities


def make_handler(capabilities: dict, token: str):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass  # keep stdout/stderr quiet; Rust only reads HTTP responses

        def _send_json(self, status: int, payload: dict):
            body = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if self.path == "/health":
                self._send_json(200, {"status": "ok"})
            else:
                self._send_json(404, {"error": "not found"})

        def do_POST(self):
            if self.path != "/rpc":
                self._send_json(404, {"error": "not found"})
                return

            if self.headers.get("Authorization") != f"Bearer {token}":
                self._send_json(401, {"error": "unauthorized"})
                return

            length = int(self.headers.get("Content-Length", 0))
            request_id = None
            try:
                request = json.loads(self.rfile.read(length))
                request_id = request.get("id")
                method = request["method"]
                params = request.get("params", {})
            except Exception as exc:
                self._send_json(200, {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32700, "message": f"parse error: {exc}"},
                })
                return

            capability = capabilities.get(method)
            if capability is None:
                self._send_json(200, {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32601, "message": f"unknown capability: {method}"},
                })
                return

            try:
                result = capability.run(params)
                self._send_json(200, {"jsonrpc": "2.0", "id": request_id, "result": result})
            except Exception as exc:
                self._send_json(200, {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32000, "message": str(exc)},
                })

    return Handler


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--token", required=True)
    parser.add_argument("--ml-dir", required=True)
    args = parser.parse_args()

    capabilities = load_capabilities(Path(args.ml_dir))
    server = HTTPServer(("127.0.0.1", args.port), make_handler(capabilities, args.token))
    server.serve_forever()


if __name__ == "__main__":
    main()
