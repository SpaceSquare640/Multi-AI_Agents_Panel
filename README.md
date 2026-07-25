# Multi-AI Agents Panel

A desktop app (Windows / Linux / macOS) for running multiple local and
cloud AI agents side by side, letting them collaborate, and orchestrating
them through independent sessions or group chats.

Full project planning lives in the Obsidian vault at
`../Multi-AI Agent Panel Document/` — start at `00 Dashboard/Dashboard.md`.
Notably:

- `01 Project Overview/Vision & Goals.md` — scope, non-goals, open-source governance
- `01 Project Overview/AI Guardrails (必守規則).md` — non-negotiable rules every agent must follow
- `01 Project Overview/Tech Stack.md` — language/technology choices and why
- `03 Development Notes/Architecture.md` — module breakdown
- `03 Development Notes/CI-CD Pipeline.md` — how releases are built

## Status

Early scaffold only — no working features yet. See the vault's `Roadmap.md`
and `Backlog.md` for what's next.

## Stack

- **Shell / core**: Rust + [Tauri](https://tauri.app) (`src-tauri/`)
- **UI**: TypeScript + React (`src/`)
- **Skills**: Python, run out-of-process (`skills/`)

## Development

```bash
npm install
npm run tauri dev
```

## License

Not yet chosen — see the vault's open-source governance checklist.
