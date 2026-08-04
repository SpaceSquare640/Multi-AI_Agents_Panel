"""Raffle winner picker: picks random winners (and optional runner-ups)
from a list of entries.

Adapted from the "raffle-winner-picker" Claude Agent Skill — the original
source `SKILL.md` is copied into this directory alongside this file (not
just referenced by an external path) for provenance. That original is a
prose document meant to be read by an LLM inside its own session (Claude
Code/Cursor/etc.); it has no runnable code of its own, so there was
nothing to literally copy for the executable part — this `skill.py` is a
from-scratch re-implementation of the behavior `SKILL.md` describes
(cryptographically random selection, exclusions, weighting, runner-ups,
optional seed for reproducibility), written to this app's `run(payload)`
Python-skill contract so it can be called by an Agent through
`skill_manager::invoke_skill` — no LLM involved in the selection itself,
it is plain deterministic-when-seeded random sampling.
"""

import random


def _entry_name(entry):
    return entry["name"] if isinstance(entry, dict) else str(entry)


def run(payload):
    entries = payload.get("entries")
    if not entries:
        raise ValueError("'entries' must be a non-empty list")

    exclude = set(payload.get("exclude", []))
    pool = [e for e in entries if _entry_name(e) not in exclude]
    if not pool:
        raise ValueError("no entries remain after applying 'exclude'")

    count = int(payload.get("count", 1))
    runner_up_count = int(payload.get("runnerUps", 0))
    total_needed = count + runner_up_count
    if total_needed > len(pool):
        raise ValueError(
            f"requested {total_needed} winners/runner-ups but only {len(pool)} eligible entries remain"
        )

    seed = payload.get("seed")
    weights = payload.get("weights")

    if seed is not None:
        rng = random.Random(seed)
        method = f"seeded (seed={seed!r}) — reproducible for verification"
    else:
        rng = random.SystemRandom()
        method = "cryptographically random (random.SystemRandom)"

    if weights:
        # Weighted sampling without replacement: repeatedly draw one
        # winner from the remaining weighted pool, per the "weighted
        # probability based on an entries/tickets column" feature.
        remaining = list(pool)
        remaining_weights = [float(weights.get(_entry_name(e), 1)) for e in remaining]
        picked = []
        for _ in range(total_needed):
            chosen = rng.choices(remaining, weights=remaining_weights, k=1)[0]
            idx = remaining.index(chosen)
            picked.append(remaining.pop(idx))
            remaining_weights.pop(idx)
    else:
        picked = rng.sample(pool, total_needed)

    winners = picked[:count]
    runner_ups = picked[count:]

    return {
        "winners": winners,
        "runnerUps": runner_ups,
        "method": method,
        "eligibleCount": len(pool),
    }
