# Contributing

Thanks for considering a contribution to Multi-AI Agents Panel.

## Project structure

- `src-tauri/` — Rust core (Tauri backend): agent dispatch, storage, guardrails, file access, skills bridge, session/group-chat logic.
- `src/` — React/TypeScript UI.
- `skills/` — Python skills, run out-of-process via the JSON-RPC bridge (`skills/_bridge.py`).

Design documents (architecture, error codes, session types, orchestration design, etc.) live in the Obsidian vault next to this repo, not in `Source_Code`. If you're proposing a design change, check there first — several decisions (Fallback ordering, Guardrails enforcement points, Session Types' conflict-resolution rules, etc.) were made deliberately and are documented with their reasoning.

## Setup

```bash
npm install
npm run tauri dev
```

Rust tests:

```bash
cd src-tauri
cargo test
```

Frontend tests (`vitest`, pure-logic unit tests only — no component/DOM
testing set up yet):

```bash
npm run test
```

Some tests are `#[ignore]`d because they call real provider APIs (OpenRouter, Anthropic) or spawn the real Python skill bridge — they need a working API key or Python interpreter and are never run in CI. Run them explicitly with:

```bash
cargo test -- --ignored
```

Never commit real API keys, even in test code — set them via environment variables (e.g. `OPENROUTER_TEST_KEY`) when running ignored tests locally.

The `ml_engine::live` tests additionally need `sentence-transformers` installed for whichever Python interpreter is on `PATH`: `pip install -r ml/requirements.txt`.

## Guidelines

- **Tests are not optional.** New logic should ship with unit tests; anything hitting a real network call or subprocess should get a `#[ignore]`d live test in addition, not instead of, fast unit tests on the pure logic underneath it.
- **Don't fake enforcement.** This project has a hard rule (see the Guardrails design doc) against implementing a safety/consent check that looks like it works but doesn't reliably do what it claims. If you can't implement something honestly, document the limitation instead of stubbing it out silently.
- **camelCase at the Rust/TypeScript boundary.** Rust structs exposed to the frontend use `#[serde(rename_all = "camelCase")]`; keep new ones consistent.
- **Small, reviewable changes.** Prefer a focused PR over a broad one — this makes the maintainers' job (and the automated 3-platform release pipeline) easier to reason about.

## Submitting changes

1. Fork the repo and create a branch from `main`.
2. Make your change, with tests.
3. Run `cargo test` and `npm run build` locally — both must pass.
4. Open a pull request describing what changed and why.

By submitting a contribution, you agree it will be licensed under this project's [MIT License](LICENSE).

## Reporting bugs / requesting features

Open a GitHub Issue. For security-relevant reports, see [SECURITY.md](SECURITY.md) instead of a public issue.
