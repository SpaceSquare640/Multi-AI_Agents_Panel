"""JSON-RPC 2.0 bridge exposing installed skills over localhost HTTP.

Spawned as a child process by the Rust skill_manager — see
Source_Code/src-tauri/src/skill_manager/mod.rs and the design note in
Multi-AI Agent Panel Document/01 Project Overview/Tech Stack.md
("Rust 殼層 ↔ Python Skills → 本機 HTTP 服務").

Only the standard library is used so this runs with any Python 3
interpreter already on the user's machine — no pip install required.
"""

import argparse
import importlib.util
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


def load_skills(skills_dir: Path) -> dict:
    """Imports every `<skills_dir>/<name>/skill.json` + its entrypoint
    module, keyed by manifest name (== JSON-RPC method name)."""
    skills = {}
    for manifest_path in sorted(skills_dir.glob("*/skill.json")):
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        entrypoint = manifest_path.parent / manifest["entrypoint"]
        spec = importlib.util.spec_from_file_location(manifest["name"], entrypoint)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        skills[manifest["name"]] = module
    return skills


def make_handler(skills: dict, token: str):
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

            skill = skills.get(method)
            if skill is None:
                self._send_json(200, {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32601, "message": f"unknown skill: {method}"},
                })
                return

            try:
                result = skill.run(params)
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
    parser.add_argument("--skills-dir", required=True)
    args = parser.parse_args()

    skills = load_skills(Path(args.skills_dir))
    server = HTTPServer(("127.0.0.1", args.port), make_handler(skills, args.token))
    server.serve_forever()


if __name__ == "__main__":
    main()
