# Changelog

All notable changes to this project. Generated with [git-cliff](https://git-cliff.org/).

## [0.1.104-alpha] - 2026-08-22

### Changes

- Redesign(ui): eDEX-UI retheme phase 5 (Settings/Skills/Onboarding/Manual)
- Chore: bump version to 0.1.104-alpha

## [0.1.103-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.102-alpha [skip ci]
- Redesign(ui): AIControlCenter.tsx deep layout redesign, eDEX-UI panels
- Chore: bump version to 0.1.103-alpha

## [0.1.102-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.101-alpha [skip ci]
- Redesign(ui): Chat.tsx deep layout redesign, eDEX-UI terminal panels
- Chore: bump version to 0.1.102-alpha

## [0.1.101-alpha] - 2026-08-22

### Changes

- Redesign(ui): eDEX-UI retheme phase 2 (remaining 6 screens' CSS)
- Chore: bump version to 0.1.101-alpha

## [0.1.100-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.99-alpha [skip ci]
- Redesign(ui): eDEX-UI-inspired retheme, phase 1 (global + Usage)
- Chore: bump version to 0.1.100-alpha

## [0.1.99-alpha] - 2026-08-22

### Added

- Feat: add OmniRoute as a local provider (github.com/diegosouzapw/OmniRoute)

### Changes

- Chore: update CHANGELOG.md for v0.1.98-alpha [skip ci]
- Chore: bump version to 0.1.99-alpha

## [0.1.98-alpha] - 2026-08-22

### Added

- Feat: add turbovec as a native-Rust ANN vector index building block

### Changes

- Chore: update CHANGELOG.md for v0.1.97-alpha [skip ci]
- Chore: bump version to 0.1.98-alpha

## [0.1.97-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.96-alpha [skip ci]
- Chore: bump version to 0.1.97-alpha

### Documentation

- Docs: add issue/PR templates and README badges

## [0.1.96-alpha] - 2026-08-22

### Added

- Feat: add colibri as a local provider (github.com/JustVugg/colibri)

### Changes

- Chore: update CHANGELOG.md for v0.1.95-alpha [skip ci]
- Chore: bump version to 0.1.96-alpha

## [0.1.95-alpha] - 2026-08-22

### Changes

- Chore: bump version to 0.1.95-alpha

### Fixed

- Fix(ci): run CHANGELOG.md commit step under bash, not the default pwsh

## [0.1.94-alpha] - 2026-08-22

### Changes

- Chore: manually backfill CHANGELOG.md for v0.1.92-alpha and v0.1.93-alpha
- Guardrails: add Llama Guard 3 classifier building block (staged, unwired)
- Chore: bump version to 0.1.94-alpha

### Documentation

- Docs: mark i18n conversion complete across all 7 screens

### Fixed

- Fix(ci): retry CHANGELOG.md push with rebase on race against main

## [0.1.93-alpha] - 2026-08-22

### Changes

- I18n: convert AIControlCenter.tsx to react-i18next (7th, final slice)
- Chore: bump version to 0.1.93-alpha

### Documentation

- Docs: update CONTRIBUTING.md i18n status for Chat.tsx conversion

## [0.1.92-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.91-alpha [skip ci]
- I18n: convert Chat.tsx to react-i18next (6th slice)
- Chore: bump version to 0.1.92-alpha

## [0.1.91-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.90-alpha [skip ci]
- Convert Manual.tsx to i18n as the fifth slice, search matches translated text

## [0.1.90-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.89-alpha [skip ci]
- Convert Onboarding.tsx to i18n as the fourth slice

## [0.1.89-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.88-alpha [skip ci]
- Convert Usage.tsx to i18n as the third slice

## [0.1.88-alpha] - 2026-08-22

### Changes

- Chore: update CHANGELOG.md for v0.1.87-alpha [skip ci]
- Convert Skills.tsx to i18n as the second slice

## [0.1.87-alpha] - 2026-08-21

### Added

- Add i18n infrastructure (react-i18next), convert Settings.tsx as first slice

### Changes

- Chore: update CHANGELOG.md for v0.1.86-alpha [skip ci]

## [0.1.86-alpha] - 2026-08-21

### Added

- Add missing unit tests for bridge_support.rs
- Add cargo-audit dependency vulnerability check to CI
- Add actionlint CI job to catch workflow config bugs like today's
- Add Dependabot config for cargo, npm, and github-actions ecosystems
- Add automated secret scanning (gitleaks) to CI
- Implement Guardrails E9004 (role-identity impersonation screen)
- Add automated CHANGELOG.md generation via git-cliff

### Changed

- Rename unified data root to ~/MultiAIAgentsPanel/ with named subfolders

### Changes

- Extend bundled Python runtime to macOS and Linux for the Skills bridge
- Update .gitignore pattern for vendored-Python .gitkeep placeholders
- Scope Release builds to Windows only, per explicit user decision
- Silence a false-positive shellcheck warning the new actionlint job found
- Chore: update CHANGELOG.md for v0.1.84-alpha [skip ci]
- Chore: update CHANGELOG.md for v0.1.85-alpha [skip ci]

### Fixed

- Fix CI break: keep vendored-Python resource dirs present via .gitkeep
- Fix Linux Release build: install libfuse2 so linuxdeploy can run
- Fix Linux Release build: extract-and-run linuxdeploy instead of FUSE-mounting it
- Fix doc claims stale after the Windows-only Release scope decision
- Fix rustdoc bare-URL warnings, gate cargo doc in CI
- Fix invalid Swatinem/rust-cache input, restoring actual CI caching
- Fix invalid shellcheck directive syntax (previous fix didn't actually fix it)
- Fix unsafe interpolation of changelog content into gh release edit
- Fix release notes step: run before committing CHANGELOG.md, not after
- Fix release notes step running as PowerShell instead of bash

## [0.1.65-alpha] - 2026-08-20

### Added

- Add a focus trap to the Onboarding modal
- Add aria-label to every bare "×" icon button

### Changes

- Set up vitest and add the first frontend unit tests
- Extract and test Skills.tsx's grant-aggregation logic
- Extract and test Manual.tsx's search-filter logic
- Actually generate and attach SHA256 checksums to releases
- Deduplicate find_python() into a shared python_env module
- Update README's stale feature list and document frontend testing
- Include the ML Engine bridge in SECURITY.md's stated scope
- Deduplicate free_local_port(); rename python_env.rs to bridge_support.rs
- Wire cargo clippy into CI to catch regressions on the just-fixed lints
- Bundle a portable Python runtime for the Skills bridge on Windows

### Fixed

- Fix Onboarding modal ignoring the Light theme entirely
- Fix error/warning banners ignoring the Light theme
- Fix .acc-error ignoring the Light theme (same bug as .chat-error)
- Fix all cargo clippy warnings

## [0.1.50-alpha] - 2026-08-19

### Added

- Implement the Global Settings screen (theme toggle, language, about)
- Implement the Skills management screen (read-only overview)
- Implement the Usage dashboard screen (call counts only)
- Implement Onboarding's forced Guardrails summary step
- Implement the built-in User Manual (searchable, in-app)
- Add a soft call-count budget warning to the Usage dashboard

### Changes

- Apply E6004 local→cloud boundary check to end_group_chat_meeting
- Replace string matching with a closed Provider enum in dispatch_one
- Show error codes as their own chip with a copy-details button

## [0.1.42-alpha] - 2026-08-16

### Added

- Add cross-provider fallback chain
- Add Track A of the Game-Playing Agent: LLM vision loop with real input automation
- Add Track B's first pipeline stage: record human demonstrations
- Add local→cloud boundary confirmation to Group Chat (E6004)

### Fixed

- Fix v0.1.37-alpha CI failures: bump xcap, add libxdo-dev on Linux
- Fix Linux CI: add libpipewire-0.3-dev for xcap's Wayland backend
- Fix Linux CI: add libgbm-dev for xcap's DRM/GBM backend

## [0.1.35-alpha] - 2026-08-05

### Changes

- Give ProviderError real Error Code Registry codes, not just text

## [0.1.34-alpha] - 2026-08-05

### Added

- Add batch API key import via file picker
- Add Ollama model storage guidance in AI Control Center
- Add a real live test proving custom-skill import is actually callable
- Add export/import (JSON) for custom role templates
- Add live OpenRouter model search with real USD pricing
- Add a real live test proving the OpenRouter fetch actually works
- Add a live test proving the cache warms after a real OpenRouter fetch
- Add real streaming progress for Ollama model pulls

### Changes

- Stop role template selection from silently overwriting a manual provider choice
- Log every fallback attempt into usage_log, not just the chain's final key
- Unify data storage into ~/MultiAIAgentsPanel-Data/ and add custom Skill import
- Prove a Group Chat cross-agent violation still gets blocked by Guardrails

## [0.1.23-alpha] - 2026-08-04

### Added

- Add generic Skill UI to Chat, and a real ported external Skill
- Add ML Engine v1: real semantic search over granted files
- Add Chat UI for semantic search (ML Engine v1 frontend)
- Implement File Access grant sharing for Group Chat
- Add "continue N turns" to Group Chat, backed by the existing E6001 cap
- Add a Role Template editor and management list

### Changes

- Wire semantic search into Group Chat

### Fixed

- Fix privacy bug: Group Chat semantic index leaked private files

## [0.1.15-alpha] - 2026-07-30

### Added

- Feat: File Access — folder grants + @file: references in chat
- Feat: Fallback across multiple keys per provider, coded E3001 on total failure
- Feat: Role Templates ("1 人公司") — apply a system prompt at agent creation
- Add multi-session parallel chat tabs and concurrency tests
- Add Group Chat with round-robin turn-taking (dev order step 12)
- Add OpenAI provider adapter

### Changes

- Wire up the Python skill bridge (dev order step 11)
- Polish pass: open-source governance docs, agent key pinning, CI/CD fixes

## [0.1.7-alpha] - 2026-07-29

### Added

- Feat: add OpenRouter provider adapter, verified against a live free model
- Feat: AI Control Center page — API keys, cloud/local model management, usage
- Feat: minimal chat UI — independent session, end-to-end with a real agent
- Feat: enforce AI Guardrails in the chat path (category 2, E9002)

## [0.1.3-alpha] - 2026-07-25

### Added

- Feat: implement Storage and Key Vault, wire storage into app startup
- Feat: Agent Manager with a unified Provider trait, first adapter (Anthropic)

## [0.1.1-alpha] - 2026-07-25

### Changes

- Initial scaffold: Tauri + React/TS shell, Rust module skeleton, skills dir, CI/release workflows
- Ci: publish releases directly instead of as drafts
- Chore: mark version as alpha

### Fixed

- Fix: correct tauri-action ref and bump to v0.1.1

