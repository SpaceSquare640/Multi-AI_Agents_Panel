# Contributing

Thanks for considering a contribution to Multi-AI Agents Panel.

## Project structure

- `src-tauri/` — Rust core (Tauri backend): agent dispatch, storage, guardrails, file access, skills bridge, session/group-chat logic.
- `src/` — React/TypeScript UI. `src/locales/` holds i18n translation files (see "Translations" below).
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

## Translations (i18n)

The platform is decided — [Weblate](https://weblate.org/) — but community
translation hasn't started yet (no Weblate project has been set up; it needs
this repo to be public first, and the maintainer's own research recommends
waiting until UI text is relatively stable to avoid translator churn, see the
i18n research note in the Obsidian vault). This is a deliberately narrow
first slice, not full coverage:

- `src/i18n.ts` wires up [react-i18next](https://react.i18next.com/).
- `src/locales/en/translation.json` is the only real locale — the source of
  truth. `Settings.tsx`, `Skills.tsx`, `Usage.tsx`, `Onboarding.tsx`,
  `Manual.tsx`, and `Chat.tsx` have been converted to
  `useTranslation()`/`t(...)` so far. **`AIControlCenter.tsx` is the only
  remaining screen** still using plain hardcoded English strings.
  Don't assume `t(...)` is wired up anywhere it isn't just because the
  infra exists — check whether the specific screen you're touching has
  been converted yet.
- When Weblate setup does happen, it'll point at the `src/locales/*/translation.json` file-mask pattern (no repo-committed Weblate config file is needed for this — component setup happens in Weblate's own dashboard).
- The `LANGUAGES` list in `Settings.tsx` intentionally shows non-English
  languages as "coming soon" rather than offering them — picking one would
  silently show untranslated English UI, which is worse than not offering
  the choice at all.

## Guidelines

- **Tests are not optional.** New logic should ship with unit tests; anything hitting a real network call or subprocess should get a `#[ignore]`d live test in addition, not instead of, fast unit tests on the pure logic underneath it.
- **Don't fake enforcement.** This project has a hard rule (see the Guardrails design doc) against implementing a safety/consent check that looks like it works but doesn't reliably do what it claims. If you can't implement something honestly, document the limitation instead of stubbing it out silently.
- **camelCase at the Rust/TypeScript boundary.** Rust structs exposed to the frontend use `#[serde(rename_all = "camelCase")]`; keep new ones consistent.
- **Small, reviewable changes.** Prefer a focused PR over a broad one — this makes the maintainers' job (and the automated release pipeline) easier to reason about.
- **`CHANGELOG.md` is auto-generated, don't hand-edit it.** Every tag push regenerates it from the git log via [git-cliff](https://git-cliff.org/) (config: `cliff.toml`) and commits it back to `main`. Commit *subject lines* end up in it verbatim, so write them for a changelog reader, not just for `git log` — a subject like "Add X" or "Fix Y" reads fine; something like "wip" or "more fixes" doesn't. There's no enforced Conventional Commits prefix requirement (`feat:`/`fix:`/etc.) — `cliff.toml` buckets unprefixed commits into a generic "Changes" section rather than dropping them, so it's a nice-to-have, not a lint rule.

## Submitting changes

1. Fork the repo and create a branch from `main`.
2. Make your change, with tests.
3. Run `cargo test` and `npm run build` locally — both must pass.
4. Open a pull request describing what changed and why.

By submitting a contribution, you agree it will be licensed under this project's [MIT License](LICENSE).

## Reporting bugs / requesting features

Open a GitHub Issue. For security-relevant reports, see [SECURITY.md](SECURITY.md) instead of a public issue.
