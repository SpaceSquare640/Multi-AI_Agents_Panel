# Security Policy

## Supported versions

This project is in **Alpha** (see version tags, `-alpha` suffix). There is
only one supported line: the latest release. Security fixes land on `main`
and go out in the next release, there is no backport policy yet.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Instead, use GitHub's private vulnerability reporting for this repository
(Security tab → "Report a vulnerability"). If that's unavailable, open an
issue asking a maintainer to contact you privately, without describing the
vulnerability itself.

Please include:

- A description of the vulnerability and its impact
- Steps to reproduce (a minimal repro is very helpful)
- The version/commit you tested against

We'll acknowledge reports as soon as we can and keep you updated as we work
on a fix. Please give us reasonable time to address the issue before any
public disclosure.

## What's in scope

- Anything that lets an Agent bypass the Guardrails checks (`guardrails`
  module — absolute-prohibition content screening, prompt/tool-injection
  screening)
- Anything that lets an Agent read files outside its explicitly granted
  folders (`file_access` module), or call a Skill it wasn't granted access
  to (`skill_manager` module)
- API keys or other secrets leaking to disk unencrypted, to logs, or to a
  provider they weren't intended for
- The local Skill bridge (`skills/_bridge.py`) accepting requests from
  anything other than the app's own Rust process (it's bound to localhost
  with a random per-launch bearer token — a bypass of that isolation is a
  valid report)

## What's explicitly out of scope (for now)

This is Alpha software with known, documented limitations — these are
tracked as design follow-ups, not vulnerabilities:

- The Guardrails content screens are keyword-based, not a real
  classification model — they will miss creatively-phrased attempts. This
  is a known limitation, documented in the `guardrails` module itself.
- There is no sandboxing between Skills — any installed Skill runs with the
  same privileges as the Python bridge process.

If you're unsure whether something is in scope, report it anyway — we'd
rather triage a false positive than miss a real issue.
