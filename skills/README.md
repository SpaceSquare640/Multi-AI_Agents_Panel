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
