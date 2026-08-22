"""Track B (Deep RL) — recording stage. See
`Multi-AI Agent Panel Document/04 Agents & Orchestration/Game-Playing Agent Design.md`
section 4, step 1 ("record"): captures a human demonstration — periodic
screen frames + a synchronized log of mouse/keyboard input events —
saved to a session folder. This is the first pipeline stage; later
stages (label/train-bc/train-rl/play) are not implemented yet (see the
design doc's "尚未定案" section) and this module does not pretend they
are.

Standalone Python CLI, not the JSON-RPC skills/ml_engine bridge pattern
— see ADR 0005 for why: this is a long-running background job Rust
starts/monitors/stops, not a request-response call.

The pure bookkeeping here (session folder naming, frame filenames,
event serialization, `Recorder`'s frame/event counting) is separated
from the actual OS-level screen/input capture (which lives in `cli.py`
and is injected into `Recorder` rather than imported directly), so this
module is unit-testable without a real screen or input device — see
`test_record.py`.
"""

import json
import time
from pathlib import Path


def session_dir(output_dir, session_name: str) -> Path:
    """The folder one recording session's frames + events.jsonl live in."""
    return Path(output_dir) / session_name


def frame_filename(index: int) -> str:
    """Zero-padded so frames sort correctly by filename alone (plain
    string sort, not just numeric), regardless of how many frames a
    session ends up with."""
    return f"frame_{index:06d}.png"


def serialize_event(event_type: str, timestamp: float, **fields) -> str:
    """One line of `events.jsonl` — a single mouse/keyboard event with
    its capture-relative timestamp (seconds since the recording
    started), JSON-serialized. Pure function, testable without a real
    input listener."""
    record = {"type": event_type, "t": timestamp, **fields}
    return json.dumps(record)


class Recorder:
    """Owns one recording session: writes numbered PNG frames + an
    `events.jsonl` log to `session_dir(output_dir, session_name)`.

    `capture_frame` is injected (a `Callable[[Path], None]` that writes
    a screenshot to the given path) rather than imported directly, so
    this class's frame-numbering/event-logging bookkeeping is testable
    with a fake capture function — no real screen access needed for the
    tests, only for the real CLI (`cli.py`).
    """

    def __init__(self, output_dir, session_name: str, capture_frame):
        self.dir = session_dir(output_dir, session_name)
        self.dir.mkdir(parents=True, exist_ok=True)
        self._capture_frame = capture_frame
        self.frame_count = 0
        self._events_path = self.dir / "events.jsonl"
        self._start_time = time.monotonic()

    def capture_one_frame(self) -> Path:
        path = self.dir / frame_filename(self.frame_count)
        self._capture_frame(path)
        # Logged as a "frame" event alongside input events (not a
        # separate file) so the label stage only has to read one
        # timeline to know both when each frame was captured and when
        # each input event happened — actual capture timing can drift
        # from the requested fps (scheduling jitter, slow disk, etc.),
        # so this is the real timestamp, not `index / fps`.
        self.log_event("frame", index=self.frame_count, filename=path.name)
        self.frame_count += 1
        return path

    def log_event(self, event_type: str, **fields) -> None:
        elapsed = time.monotonic() - self._start_time
        line = serialize_event(event_type, elapsed, **fields)
        with open(self._events_path, "a", encoding="utf-8") as f:
            f.write(line + "\n")
