"""Placeholder skill: echoes its input. Not yet wired into the JSON-RPC bridge."""


def run(payload: dict) -> dict:
    return {"echo": payload}
