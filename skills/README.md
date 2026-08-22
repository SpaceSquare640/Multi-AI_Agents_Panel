# Skills (Python)

Skills run as a separate Python process, spawned by the Rust core, and talk
to it over a local-only JSON-RPC 2.0 HTTP service (loopback + per-launch
token). See the design notes in the Obsidian vault:

- `Multi-AI Agent Panel Document/01 Project Overview/Tech Stack.md` — cross-language communication decisions
- `Multi-AI Agent Panel Document/03 Development Notes/Architecture.md` — Skill Manager

Each skill is a folder under `skills/` and is only reachable by an agent
that has been explicitly granted access to it (per-agent allowlist, no
inheritance — see `Agent Registry.md`).

`example_skill/` is a placeholder showing the expected shape; it is not
wired into the Rust side yet.

## Permissions (npm-style, default-deny)

`skill.json` may declare a `"permissions"` array naming the capabilities
a skill needs beyond pure computation on its `payload`:

- `"filesystem"` — needed to call `open()`
- `"network"` — needed to open a socket (`socket.socket`/`socket.create_connection`)

Omitting the field (or leaving it `[]`) means neither is available — this
is enforced for real by `_bridge.py`'s `sandbox`, not just a label: a
skill that doesn't declare a permission gets a `PermissionError` the
moment it tries to use it, surfaced to the Rust side as `SkillError::PermissionDenied`
(E4005). Both bundled skills declare `"permissions": []` since they only
transform their input.
