# Multi-AI Agents Panel

[![CI](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/ci.yml/badge.svg)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/ci.yml)
[![Release](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/release.yml/badge.svg)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/SpaceSquare640/Multi-AI_Agents_Panel)](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/releases)

A desktop app for running several local and cloud AI agents side by
side — each in its own session, or together in a Group Chat — with
Skills, MCP tools, file access, and memory they can draw on, all gated
by non-negotiable Guardrails.

**Providers**: Anthropic, OpenAI, OpenRouter, Ollama, [colibrì](https://github.com/JustVugg/colibri)
**Status**: v1.0.0, General Availability. Windows-only installers for now.

## Get it

Download the latest installer from
[Releases](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel/releases).
Installers are unsigned — see [SECURITY.md](SECURITY.md).

## Run from source

```bash
npm install
npm run tauri dev
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for tests and how to submit changes.

## Stack

Rust + [Tauri](https://tauri.app) (`src-tauri/`) · TypeScript + React
(`src/`) · Python Skills, run out-of-process (`skills/`)

## License

[MIT](LICENSE).
