# Multi-AI Agents Panel

A desktop app for running multiple local and cloud AI agents side by
side, letting them collaborate, and orchestrating them through
independent sessions or group chats.

Currently ships **Windows-only** installers — the maintainer has no way
to verify Linux/macOS builds on real hardware, so the Release pipeline
was scoped down to Windows for now (see the Backlog in the vault for the
full history, including an unresolved upstream Tauri Linux AppImage
bundling bug). The codebase itself is still cross-platform (Tauri +
Rust), and the macOS/Linux-specific config isn't deleted, just unused.

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

- Independent Sessions with multiple providers (Anthropic, OpenAI,
  OpenRouter, Ollama), each with real fallback across multiple keys per
  provider, plus cross-provider fallback chains (e.g. Anthropic fails →
  fall through to OpenRouter)
- Multiple Independent Sessions open and chatting in parallel
- Group Chat: round-robin turn-taking, `@mention` interruption, a loop
  safety-net, meeting summarization, and an explicit confirmation step
  before a local Agent's reply is ever sent to a cloud provider
- Role Templates (10 built-in "1人公司" roles + user-defined custom ones,
  with export/import for sharing)
- File Access with explicit per-folder, per-agent consent
- A Python Skills bridge (JSON-RPC over localhost) with per-agent
  allowlists, plus a separate ML Engine bridge for semantic search over
  granted files
- Guardrails: absolute-prohibition content screening and
  prompt/tool-injection screening, enforced inline (not opt-in) at every
  point an Agent can act
- Live OpenRouter model catalog (real pricing, 24h cache) and Ollama
  model management (search, install with streaming progress)
- Usage dashboard with a soft call-count budget warning
- Dark/Light/System theme, first-launch Guardrails summary, and an
  in-app searchable user manual
- An experimental Game-Playing Agent (`AI Control Center` →
  "Game-Playing Agent") — local vision-model screenshot/action loop;
  research-grade, off by default, see the vault's Backlog for its
  current limitations

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
