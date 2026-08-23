# Multi-AI Agents Panel

[![CI](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/ci.yml/badge.svg)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/ci.yml)
[![Release](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/release.yml/badge.svg)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/SpaceSquare640/Multi-AI_Agents_Panel)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/releases)

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

**v1.0.0 — General Availability.** No more `-alpha`/`-beta` tags; the
alpha→beta→GA transition was the maintainer's own call (see the vault's
`CI-CD Pipeline.md`). Releases are still unsigned (see below), and the
app continues to evolve — check the vault's `Roadmap.md`/`Backlog.md`
for what's next. Implemented so far:

- Independent Sessions with multiple providers (Anthropic, OpenAI,
  OpenRouter, Ollama, [colibrì](https://github.com/JustVugg/colibri)),
  each with real fallback across multiple keys per provider, plus
  cross-provider fallback chains (e.g. Anthropic fails → fall through to
  OpenRouter)
- Multiple Independent Sessions open and chatting in parallel, each
  keeping its own state — switching tabs never resets or re-fetches
  another one
- Group Chat: round-robin turn-taking, `@mention` interruption, a loop
  safety-net, meeting summarization, and an explicit confirmation step
  before a local Agent's reply is ever sent to a cloud provider
- Role Templates (10 built-in "1人公司" roles + user-defined custom ones,
  with export/import for sharing)
- File Access with explicit per-folder, per-agent consent
- A Python Skills bridge (JSON-RPC over localhost) with per-agent
  allowlists and npm-style default-deny permissions (`skill.json`
  declares what filesystem/network access it needs — anything
  undeclared is actually blocked, including at module-import time, not
  just when the skill runs)
- MCP (Model Context Protocol) client support — connect to local MCP
  servers over stdio, with the same per-agent allowlist and
  Guardrails-screening model as Skills, plus tool-metadata screening
  against tool-poisoning
- Agent function calling (Anthropic) — an Agent can decide for itself
  when to call a granted Skill mid-reply, through the exact same
  Guardrails/permission checks as a human clicking "Run"
- A Task DAG orchestrator — define a dependency graph of Agent calls and
  run it in topological order, with each node's output flowing into the
  next
- Long-term, cross-session memory per Agent, plus a separate global
  "Custom Instructions" block (like Claude's own) always included as
  context for every Agent on every message
- A separate ML Engine bridge for semantic search over granted files
- Guardrails: absolute-prohibition content screening and
  prompt/tool-injection screening, enforced inline (not opt-in) at every
  point an Agent can act, with an optional Llama Guard 3 second-pass
  classifier
- Live OpenRouter model catalog (real pricing, 24h cache) and Ollama
  model management (search, install with streaming progress), with
  hardware-aware model recommendations via
  [llmfit](https://github.com/AlexsJones/llmfit) (which local models
  will actually run well on your RAM/CPU/GPU)
- Usage dashboard with a soft call-count budget warning and estimated
  cost (OpenRouter calls with known pricing)
- 7 interface languages (English, 繁體中文, 简体中文, Français, Deutsch,
  日本語, 한국어) — currently wired up for the Settings screen, with the
  rest of the app following
- Dark/Light/System theme, first-launch Guardrails summary, and an
  in-app searchable user manual
- An experimental Game-Playing Agent (`AI Control Center` →
  "Game-Playing Agent") — local vision-model screenshot/action loop with
  basic anti-detection jitter (randomized timing, multi-step mouse
  movement); research-grade, off by default, see the vault's Backlog for
  its current limitations
- An experimental Deep RL pipeline (Track B) for the Game-Playing Agent
  — demonstration recording, labeling, and behavior-cloning training;
  framework-level, ships with no pretrained models or demonstration data

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
