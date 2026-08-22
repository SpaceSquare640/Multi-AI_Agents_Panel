"""Track B (Deep RL) — labeling stage. See
`Multi-AI Agent Panel Document/04 Agents & Orchestration/Game-Playing Agent Design.md`
section 4, step 2 ("label"): turns one recorded session (frames +
`events.jsonl`, see `record.py`) into supervised-learning examples for
the next stage (`train-bc`, not implemented yet) — one `(frame, action)`
pair per frame, where `action` is whichever input event happened soon
enough after that frame to plausibly be the human reacting to what that
frame showed, or `{"type": "wait"}` if nothing did.

Pure, deterministic, and fully testable against synthetic events —
labeling is just matching timestamps that already exist in
`events.jsonl`, so this doesn't need a real recorded human demonstration
session to verify. It does need one to actually produce a training set,
which this repo doesn't have yet (see the vault's Daily Log "待釐清"
section) — this module is the honest next increment given that: the
matching logic is real and tested, `train-bc` (the stage that would
consume its output) is future work once real demonstration data exists.
"""

import json
from pathlib import Path

# How long after a frame an input event can still count as "the human's
# reaction to that frame" — chosen as a plausible human-reaction-time
# upper bound, not tuned against real data (there isn't any yet). Frames
# are typically ~0.5s apart at the default 2fps recording rate, so this
# also naturally caps how many frames one action can be attributed to.
DEFAULT_MATCH_WINDOW_SECONDS = 1.0


def read_events(events_path) -> list:
    """Reads `events.jsonl` into a list of dicts, in file order (which is
    always chronological — `Recorder.log_event` only ever appends)."""
    events = []
    with open(events_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                events.append(json.loads(line))
    return events


def _to_action(event: dict) -> dict:
    """Converts one raw input event (as logged by `record.py`'s
    `log_event`) into the label's `action` shape. Only `click`/`key`
    events are ever passed in (see `label_frames`'s filtering) — `frame`
    events never reach here."""
    if event["type"] == "click":
        return {"type": "click", "x": event["x"], "y": event["y"]}
    return {"type": "key", "key": event["key"]}


def label_frames(events: list, window_seconds: float = DEFAULT_MATCH_WINDOW_SECONDS) -> list:
    """For every `frame` event in `events`, finds the earliest `click`/
    `key` event that happened at or after that frame's timestamp and
    within `window_seconds` of it, and labels the frame with that
    action — or `{"type": "wait"}` if none qualifies. `events` need not
    be sorted by the caller (this sorts on `t` itself, defensively —
    `record.py` always appends in order, but a hand-edited or
    concatenated `events.jsonl` might not be).

    One action can label more than one frame (e.g. a slow click after
    several idle frames labels only the frame(s) within its window, but
    two frames close together can share the same subsequent action) —
    deliberately not "consumed" once used, since a single human action
    is a real, continuing reaction across every frame it's still the
    most recent explanation for within the window.
    """
    ordered = sorted(events, key=lambda e: e["t"])
    frames = [e for e in ordered if e["type"] == "frame"]
    actions = [e for e in ordered if e["type"] in ("click", "key")]

    labels = []
    for frame in frames:
        match = next((a for a in actions if frame["t"] <= a["t"] <= frame["t"] + window_seconds), None)
        action = _to_action(match) if match else {"type": "wait"}
        labels.append({"frame": frame["filename"], "action": action})
    return labels


def label_session(session_dir, window_seconds: float = DEFAULT_MATCH_WINDOW_SECONDS) -> list:
    """Labels a whole recorded session directory (as produced by
    `record.py`'s `Recorder`) and writes the result to
    `<session_dir>/labels.jsonl`, one JSON object per line matching
    `label_frames`'s output shape. Returns the labels list as well, so
    callers (tests, the CLI) don't have to re-read the file they just
    wrote."""
    session_dir = Path(session_dir)
    events = read_events(session_dir / "events.jsonl")
    labels = label_frames(events, window_seconds)

    labels_path = session_dir / "labels.jsonl"
    with open(labels_path, "w", encoding="utf-8") as f:
        for label in labels:
            f.write(json.dumps(label) + "\n")
    return labels
