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

**Alpha** (`-alpha` version tags). Core features work end-to-end and are
covered by tests, but interfaces and storage formats may still change
without a migration path. Implemented so far:

- Independent Sessions with multiple providers (Anthropic, OpenRouter,
  Ollama), each with real fallback across multiple keys per provider
- Multiple Independent Sessions open and chatting in parallel
- Group Chat: round-robin turn-taking, `@mention` interruption, a loop
  safety-net, and meeting summarization
- Role Templates (10 built-in "1人公司" roles + user-defined custom ones)
- File Access with explicit per-folder, per-agent consent
- A Python Skills bridge (JSON-RPC over localhost) with per-agent allowlists
- Guardrails: absolute-prohibition content screening and
  prompt/tool-injection screening, enforced inline (not opt-in) at every
  point an Agent can act

See the vault's `Roadmap.md` and `Backlog.md` for what's next and what's
deliberately not built yet, with reasoning.

## Stack

- **Shell / core**: Rust + [Tauri](https://tauri.app) (`src-tauri/`)
- **UI**: TypeScript + React (`src/`)
- **Skills**: Python, run out-of-process (`skills/`)

## Development

```bash
npm install
npm run tauri dev
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to run tests and submit changes.

## Security

See [SECURITY.md](SECURITY.md) for how to report a vulnerability.

## License

[MIT](LICENSE).
