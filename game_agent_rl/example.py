"""Generates a small, fully synthetic example session — NOT a real human
demonstration recording, just a session folder shaped exactly like what
`record.py` produces (numbered PNG frames + `events.jsonl`), so someone
can run `label`/`train-bc` against real files and see the pipeline
actually work end to end without recording their own gameplay first.

This is a different thing from the "no demonstration data shipped"
decision (`2026-08-21`): that decision is about not bundling real human
gameplay recordings or trained weights, which would be a real
copyright/size/provenance concern for an open-source repo. A handful of
solid-color placeholder frames with hand-crafted click/key timings isn't
demonstration data at all — there's no game, no human, nothing to have
provenance — it's a format example, the same role a "hello world" plays
for a compiler.

Deterministic: the same session name and frame count always produce
byte-identical output, so this is reproducible and diffable rather than
opaque generated noise.
"""

from pathlib import Path

from .record import frame_filename, serialize_event, session_dir


def generate_example_session(output_dir, session_name: str = "example") -> Path:
    """Writes a synthetic session with a handful of frames: some followed
    by a click, one followed by a key press, and at least one followed
    by nothing (so `label.py`'s "wait" case has something real to label
    too) — exercising every action type `label.py`/`train_bc.py` know
    about, not just the easy case. Returns the session's directory.
    """
    from PIL import Image  # lazy: only `example`/`train-bc` need Pillow, not `record`/`label`

    # Explicit (frame_t, action) pairs rather than accumulated
    # increments — this makes the ~2s gap between frames, and the
    # separation between an idle frame and its neighbors' actions,
    # deliberate and easy to verify by eye, rather than emerging by
    # accident from a running total. Idle frames sit >1s away from any
    # action on either side — `label.py`'s default 1.0s window — so they
    # come out "wait" rather than accidentally borrowing a neighboring
    # frame's action.
    frame_specs = [
        (0.0, {"type": "click", "x": 120, "y": 340}, 0.1),
        (2.0, {"type": "key", "key": "space"}, 0.1),
        (4.0, None, None),
        (6.0, {"type": "click", "x": 400, "y": 60}, 0.1),
        (8.0, None, None),
        (10.0, {"type": "key", "key": "'a'"}, 0.1),
    ]

    dir_ = session_dir(output_dir, session_name)
    dir_.mkdir(parents=True, exist_ok=True)

    events = []
    for index, (frame_t, action, action_delay) in enumerate(frame_specs):
        # A distinct, deterministic solid color per frame — not a real
        # screenshot, just something that's a valid, openable PNG so the
        # pipeline's real image-loading code path (label.py doesn't
        # touch images, but train_bc.py's DemonstrationDataset does) has
        # something real to decode.
        color = ((index * 37) % 256, (index * 91) % 256, (index * 149) % 256)
        Image.new("RGB", (64, 64), color=color).save(dir_ / frame_filename(index))
        events.append(serialize_event("frame", frame_t, index=index, filename=frame_filename(index)))

        if action is not None:
            action_t = frame_t + action_delay
            if action["type"] == "click":
                events.append(
                    serialize_event("click", action_t, x=action["x"], y=action["y"], button="Button.left", pressed=True)
                )
            else:
                events.append(serialize_event("key", action_t, key=action["key"]))

    with open(dir_ / "events.jsonl", "w", encoding="utf-8") as f:
        for line in events:
            f.write(line + "\n")

    return dir_
