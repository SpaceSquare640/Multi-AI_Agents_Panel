"""Example skill: echoes its input. Called through `_bridge.py` via the
Rust skill_manager's JSON-RPC bridge — see `skill_manager::invoke_skill`."""


def run(payload: dict) -> dict:
    return {"echo": payload}
