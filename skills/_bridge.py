"""JSON-RPC 2.0 bridge exposing installed skills over localhost HTTP.

Spawned as a child process by the Rust skill_manager — see
Source_Code/src-tauri/src/skill_manager/mod.rs and the design note in
Multi-AI Agent Panel Document/01 Project Overview/Tech Stack.md
("Rust 殼層 ↔ Python Skills → 本機 HTTP 服務").

Only the standard library is used so this runs with any Python 3
interpreter already on the user's machine — no pip install required.
"""

import argparse
import builtins
import importlib.util
import json
import socket
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

# npm-style default-deny sandbox: a skill gets none of these capabilities
# unless its skill.json's "permissions" array names them. Mirrors the
# Rust side's `SkillManifest::permissions` (src-tauri/src/skill_manager/mod.rs)
# — that struct just carries the declaration through to the UI; this is
# where it's actually enforced, since only the Python side can restrict
# what a skill's own code does when it runs.
KNOWN_PERMISSIONS = {"network", "filesystem"}

_real_open = builtins.open
_real_socket = socket.socket
_real_create_connection = socket.create_connection


class SkillPermissionError(Exception):
    """Raised when a skill calls a capability it didn't declare in its
    manifest's "permissions" array — a real enforcement failure, not a
    logged warning, so a misbehaving or compromised skill can't just
    quietly touch the filesystem or network because nobody's watching."""


def _blocked_open(*_args, **_kwargs):
    raise SkillPermissionError('this skill did not declare the "filesystem" permission in its skill.json')


def _blocked_socket(*_args, **_kwargs):
    raise SkillPermissionError('this skill did not declare the "network" permission in its skill.json')


class sandbox:
    """Context manager that restricts `builtins.open` and socket creation
    for the duration of one skill call, based on that skill's declared
    permissions. Not a full OS-level sandbox (a sufficiently determined
    skill could still reach the filesystem/network through other stdlib
    paths, e.g. `os.open` or `ctypes`) — it's a real default-deny gate on
    the two capabilities skills would ordinarily reach for, which is what
    npm's own `package.json` permission model covers too. Applied
    globally rather than per-module because Python has no per-module
    sandboxing primitive; safe here because the bridge's HTTPServer
    handles one request at a time (see `main`), so no other skill call
    can be mid-flight while this one's restrictions are active."""

    def __init__(self, permissions):
        self.permissions = set(permissions)

    def __enter__(self):
        if "filesystem" not in self.permissions:
            builtins.open = _blocked_open
        if "network" not in self.permissions:
            socket.socket = _blocked_socket
            socket.create_connection = _blocked_create_connection
        return self

    def __exit__(self, *_exc_info):
        builtins.open = _real_open
        socket.socket = _real_socket
        socket.create_connection = _real_create_connection
        return False


def _blocked_create_connection(*_args, **_kwargs):
    raise SkillPermissionError('this skill did not declare the "network" permission in its skill.json')


def load_skills(skills_dirs: list) -> dict:
    """Imports every `<dir>/<name>/skill.json` + its entrypoint module
    across all `skills_dirs`, keyed by manifest name (== JSON-RPC method
    name). Directories are scanned in the order given; a name that
    appears in a later directory overwrites one from an earlier
    directory — this is what lets a user-supplied custom skill directory
    (passed after the built-in one) override a built-in skill of the
    same name.

    Each entry stores both the imported `module` and its declared
    `permissions` (unknown permission strings in skill.json are ignored
    rather than rejected, so a manifest is never "invalid" just because
    it names a capability this bridge version doesn't recognize yet)."""
    skills = {}
    for skills_dir in skills_dirs:
        for manifest_path in sorted(skills_dir.glob("*/skill.json")):
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            entrypoint = manifest_path.parent / manifest["entrypoint"]
            spec = importlib.util.spec_from_file_location(manifest["name"], entrypoint)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            permissions = [p for p in manifest.get("permissions", []) if p in KNOWN_PERMISSIONS]
            skills[manifest["name"]] = {"module": module, "permissions": permissions}
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
                with sandbox(skill["permissions"]):
                    result = skill["module"].run(params)
                self._send_json(200, {"jsonrpc": "2.0", "id": request_id, "result": result})
            except SkillPermissionError as exc:
                self._send_json(200, {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32001, "message": str(exc)},
                })
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
    parser.add_argument("--skills-dir", required=True, action="append")
    args = parser.parse_args()

    skills = load_skills([Path(d) for d in args.skills_dir])
    server = HTTPServer(("127.0.0.1", args.port), make_handler(skills, args.token))
    server.serve_forever()


if __name__ == "__main__":
    main()
