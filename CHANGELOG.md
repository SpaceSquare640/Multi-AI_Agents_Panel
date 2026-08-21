# Changelog

All notable changes to this project. Generated with [git-cliff](https://git-cliff.org/).

## [0.1.82-alpha] - 2026-08-21

### Added

- Implement Guardrails E9004 (role-identity impersonation screen)

## [0.1.81-alpha] - 2026-08-21

### Added

- Add automated secret scanning (gitleaks) to CI

## [0.1.80-alpha] - 2026-08-21

### Added

- Add Dependabot config for cargo, npm, and github-actions ecosystems

## [0.1.79-alpha] - 2026-08-21

### Fixed

- Fix invalid shellcheck directive syntax (previous fix didn't actually fix it)

## [0.1.78-alpha] - 2026-08-21

### Changes

- Silence a false-positive shellcheck warning the new actionlint job found

## [0.1.77-alpha] - 2026-08-21

### Added

- Add actionlint CI job to catch workflow config bugs like today's

## [0.1.76-alpha] - 2026-08-21

### Fixed

- Fix invalid Swatinem/rust-cache input, restoring actual CI caching

## [0.1.75-alpha] - 2026-08-21

### Added

- Add cargo-audit dependency vulnerability check to CI

## [0.1.74-alpha] - 2026-08-21

### Fixed

- Fix rustdoc bare-URL warnings, gate cargo doc in CI

## [0.1.73-alpha] - 2026-08-21

### Added

- Add missing unit tests for bridge_support.rs

## [0.1.72-alpha] - 2026-08-21

### Fixed

- Fix doc claims stale after the Windows-only Release scope decision

## [0.1.71-alpha] - 2026-08-20

### Changes

- Scope Release builds to Windows only, per explicit user decision

## [0.1.70-alpha] - 2026-08-20

### Fixed

- Fix Linux Release build: extract-and-run linuxdeploy instead of FUSE-mounting it

## [0.1.69-alpha] - 2026-08-20

### Changed

- Rename unified data root to ~/MultiAIAgentsPanel/ with named subfolders

## [0.1.68-alpha] - 2026-08-20

### Fixed

- Fix Linux Release build: install libfuse2 so linuxdeploy can run

## [0.1.67-alpha] - 2026-08-20

### Changes

- Update .gitignore pattern for vendored-Python .gitkeep placeholders

### Fixed

- Fix CI break: keep vendored-Python resource dirs present via .gitkeep

## [0.1.66-alpha] - 2026-08-20

### Changes

- Extend bundled Python runtime to macOS and Linux for the Skills bridge

## [0.1.65-alpha] - 2026-08-20

### Changes

- Bundle a portable Python runtime for the Skills bridge on Windows

## [0.1.64-alpha] - 2026-08-20

### Changes

- Wire cargo clippy into CI to catch regressions on the just-fixed lints

### Fixed

- Fix all cargo clippy warnings

## [0.1.63-alpha] - 2026-08-20

### Changes

- Deduplicate free_local_port(); rename python_env.rs to bridge_support.rs

## [0.1.62-alpha] - 2026-08-20

### Changes

- Include the ML Engine bridge in SECURITY.md's stated scope

## [0.1.61-alpha] - 2026-08-20

### Changes

- Update README's stale feature list and document frontend testing

## [0.1.60-alpha] - 2026-08-20

### Changes

- Deduplicate find_python() into a shared python_env module

## [0.1.59-alpha] - 2026-08-20

### Changes

- Actually generate and attach SHA256 checksums to releases

## [0.1.58-alpha] - 2026-08-20

### Changes

- Extract and test Manual.tsx's search-filter logic

## [0.1.57-alpha] - 2026-08-20

### Changes

- Extract and test Skills.tsx's grant-aggregation logic

## [0.1.56-alpha] - 2026-08-20

### Changes

- Set up vitest and add the first frontend unit tests

## [0.1.55-alpha] - 2026-08-20

### Added

- Add aria-label to every bare "×" icon button

## [0.1.54-alpha] - 2026-08-20

### Added

- Add a focus trap to the Onboarding modal

## [0.1.53-alpha] - 2026-08-20

### Fixed

- Fix .acc-error ignoring the Light theme (same bug as .chat-error)

## [0.1.52-alpha] - 2026-08-20

### Fixed

- Fix error/warning banners ignoring the Light theme

## [0.1.51-alpha] - 2026-08-19

### Fixed

- Fix Onboarding modal ignoring the Light theme entirely

## [0.1.50-alpha] - 2026-08-19

### Added

- Add a soft call-count budget warning to the Usage dashboard

## [0.1.49-alpha] - 2026-08-19

### Added

- Implement the built-in User Manual (searchable, in-app)

## [0.1.48-alpha] - 2026-08-19

### Added

- Implement Onboarding's forced Guardrails summary step

### Changes

- Show error codes as their own chip with a copy-details button

## [0.1.46-alpha] - 2026-08-19

### Added

- Implement the Skills management screen (read-only overview)
- Implement the Usage dashboard screen (call counts only)

## [0.1.45-alpha] - 2026-08-19

### Added

- Implement the Global Settings screen (theme toggle, language, about)

## [0.1.44-alpha] - 2026-08-18

### Changes

- Replace string matching with a closed Provider enum in dispatch_one

## [0.1.43-alpha] - 2026-08-18

### Changes

- Apply E6004 local→cloud boundary check to end_group_chat_meeting

## [0.1.42-alpha] - 2026-08-16

### Added

- Add local→cloud boundary confirmation to Group Chat (E6004)

## [0.1.41-alpha] - 2026-08-16

### Added

- Add Track B's first pipeline stage: record human demonstrations

## [0.1.40-alpha] - 2026-08-15

### Fixed

- Fix Linux CI: add libgbm-dev for xcap's DRM/GBM backend

## [0.1.39-alpha] - 2026-08-15

### Fixed

- Fix Linux CI: add libpipewire-0.3-dev for xcap's Wayland backend

## [0.1.38-alpha] - 2026-08-15

### Fixed

- Fix v0.1.37-alpha CI failures: bump xcap, add libxdo-dev on Linux

## [0.1.37-alpha] - 2026-08-15

### Added

- Add Track A of the Game-Playing Agent: LLM vision loop with real input automation

## [0.1.36-alpha] - 2026-08-15

### Added

- Add cross-provider fallback chain

## [0.1.35-alpha] - 2026-08-05

### Changes

- Give ProviderError real Error Code Registry codes, not just text

## [0.1.34-alpha] - 2026-08-05

### Changes

- Prove a Group Chat cross-agent violation still gets blocked by Guardrails

## [0.1.33-alpha] - 2026-08-05

### Added

- Add real streaming progress for Ollama model pulls

## [0.1.32-alpha] - 2026-08-05

### Added

- Add a real live test proving the OpenRouter fetch actually works
- Add a live test proving the cache warms after a real OpenRouter fetch

## [0.1.31-alpha] - 2026-08-05

### Added

- Add live OpenRouter model search with real USD pricing

## [0.1.30-alpha] - 2026-08-05

### Added

- Add export/import (JSON) for custom role templates

## [0.1.29-alpha] - 2026-08-05

### Added

- Add a real live test proving custom-skill import is actually callable

## [0.1.28-alpha] - 2026-08-05

### Added

- Add Ollama model storage guidance in AI Control Center

## [0.1.27-alpha] - 2026-08-05

### Changes

- Unify data storage into ~/MultiAIAgentsPanel-Data/ and add custom Skill import

## [0.1.26-alpha] - 2026-08-05

### Changes

- Log every fallback attempt into usage_log, not just the chain's final key

## [0.1.25-alpha] - 2026-08-04

### Changes

- Stop role template selection from silently overwriting a manual provider choice

## [0.1.24-alpha] - 2026-08-04

### Added

- Add batch API key import via file picker

## [0.1.23-alpha] - 2026-08-04

### Added

- Add a Role Template editor and management list

## [0.1.22-alpha] - 2026-08-04

### Added

- Add "continue N turns" to Group Chat, backed by the existing E6001 cap

## [0.1.21-alpha] - 2026-08-04

### Fixed

- Fix privacy bug: Group Chat semantic index leaked private files

## [0.1.20-alpha] - 2026-08-04

### Changes

- Wire semantic search into Group Chat

## [0.1.19-alpha] - 2026-08-04

### Added

- Implement File Access grant sharing for Group Chat

## [0.1.18-alpha] - 2026-08-04

### Added

- Add Chat UI for semantic search (ML Engine v1 frontend)

## [0.1.17-alpha] - 2026-08-04

### Added

- Add ML Engine v1: real semantic search over granted files

## [0.1.16-alpha] - 2026-08-04

### Added

- Add generic Skill UI to Chat, and a real ported external Skill

## [0.1.15-alpha] - 2026-07-30

### Added

- Add OpenAI provider adapter

## [0.1.14-alpha] - 2026-07-29

### Changes

- Polish pass: open-source governance docs, agent key pinning, CI/CD fixes

## [0.1.13-alpha] - 2026-07-29

### Added

- Add Group Chat with round-robin turn-taking (dev order step 12)

## [0.1.12-alpha] - 2026-07-29

### Changes

- Wire up the Python skill bridge (dev order step 11)

## [0.1.11-alpha] - 2026-07-29

### Added

- Add multi-session parallel chat tabs and concurrency tests

## [0.1.10-alpha] - 2026-07-29

### Added

- Feat: Role Templates ("1 人公司") — apply a system prompt at agent creation

## [0.1.9-alpha] - 2026-07-29

### Added

- Feat: Fallback across multiple keys per provider, coded E3001 on total failure

## [0.1.8-alpha] - 2026-07-29

### Added

- Feat: File Access — folder grants + @file: references in chat

## [0.1.7-alpha] - 2026-07-29

### Added

- Feat: enforce AI Guardrails in the chat path (category 2, E9002)

## [0.1.6-alpha] - 2026-07-29

### Added

- Feat: minimal chat UI — independent session, end-to-end with a real agent

## [0.1.5-alpha] - 2026-07-29

### Added

- Feat: AI Control Center page — API keys, cloud/local model management, usage

## [0.1.4-alpha] - 2026-07-29

### Added

- Feat: add OpenRouter provider adapter, verified against a live free model

## [0.1.3-alpha] - 2026-07-25

### Added

- Feat: Agent Manager with a unified Provider trait, first adapter (Anthropic)

## [0.1.2-alpha] - 2026-07-25

### Added

- Feat: implement Storage and Key Vault, wire storage into app startup

## [0.1.1-alpha] - 2026-07-25

### Changes

- Initial scaffold: Tauri + React/TS shell, Rust module skeleton, skills dir, CI/release workflows
- Ci: publish releases directly instead of as drafts
- Chore: mark version as alpha

### Fixed

- Fix: correct tauri-action ref and bump to v0.1.1

