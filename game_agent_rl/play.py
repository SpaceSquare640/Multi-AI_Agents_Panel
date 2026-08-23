"""Track B (Deep RL) — play stage. See the Game-Playing Agent design
notes, section 4, step 5: loads a trained checkpoint (`train_bc.py` or
`train_rl.py`'s output) and runs it live — screenshot in, predicted
action out, real mouse/keyboard input executed — completing the
record -> label -> train-bc -> train-rl -> play pipeline end to end.

**Same safety posture as Track A** (`game_agent` module, Rust side):
this only ever runs when explicitly invoked from the CLI, never
automatically; the jittered timing below is the same "reduce the most
naive automation fingerprints" mitigation Track A ships, not a claim
about defeating real anti-cheat. Track A's own module doc says it
plainly and it applies here too: don't point this at a game with
server-side anti-cheat or terms that prohibit automation — that's a
choice about which game/account to run it against, not something this
code can make safe for you.

**Honest scope**: `decide_action` (the pure prediction step — frame in,
action dict out) is exactly what the tests below exercise, since it
needs no display, no real mouse/keyboard, and no trained checkpoint of
real quality to verify it produces a well-formed action from a model's
raw output. The live `run` loop that actually captures the screen and
moves the mouse is real, working code, but — like every Track B stage
so far — this repo ships no trained checkpoint and no claim that a
model trained on whatever small session a user records will play well;
it decodes exactly what the model predicts, no more, no less.
"""

import time
from pathlib import Path

import torch

from .train_bc import ACTION_TYPES, load_checkpoint, load_frame_tensor


def decide_action(model, key_vocab, frame_tensor: torch.Tensor) -> dict:
    """One forward pass -> one decoded action dict, shaped exactly like
    `label.py`'s labels so the same action vocabulary flows through the
    whole pipeline. Pure and stateless — no screen capture, no input
    execution — so it's testable against a frame tensor built any way
    (a real screenshot or a synthetic one)."""
    with torch.no_grad():
        type_logits, click_pred, key_logits = model(frame_tensor.unsqueeze(0))
    action_type = ACTION_TYPES[type_logits.argmax(dim=1).item()]

    if action_type == "click":
        x, y = click_pred[0].tolist()
        return {"type": "click", "x": round(x), "y": round(y)}
    if action_type == "key":
        key_idx = key_logits.argmax(dim=1).item()
        if key_idx >= len(key_vocab.keys):
            # The reserved "unknown key" slot — the model picked a key
            # index that wasn't in its own training vocabulary. There's
            # no real key name to map this back to, so this decays to a
            # wait rather than guessing a key to press.
            return {"type": "wait"}
        return {"type": "key", "key": key_vocab.keys[key_idx]}
    return {"type": "wait"}


def decode_key_for_input(key_name: str):
    """The inverse of `record.py`'s `str(key)` logging: `"Key.space"` ->
    `pynput.keyboard.Key.space`, `"'a'"` -> `"a"` (pynput's `Controller`
    accepts a plain one-character string for regular keys, and a `Key`
    enum member for special ones — this is what decides which shape to
    hand it)."""
    if key_name.startswith("Key."):
        from pynput.keyboard import Key

        attr = key_name.removeprefix("Key.")
        return getattr(Key, attr, None)
    return key_name.strip("'")


def jittered_interval(base_seconds: float, jitter_fraction: float, random_unit: float) -> float:
    """Same jitter shape as Track A's Rust `jittered_duration` — a
    perfectly constant tick interval is one of the most naive automation
    fingerprints, so this spreads each tick over
    `base * (1 +/- jitter_fraction)` instead. `random_unit` (expected in
    `[0, 1]`) is a parameter rather than an internal `random()` call so
    this stays pure and testable; real callers pass `random.random()`."""
    random_unit = max(0.0, min(1.0, random_unit))
    offset = (random_unit * 2.0 - 1.0) * jitter_fraction
    return max(0.0, base_seconds * (1.0 + offset))


def run(checkpoint_path, fps: float = 2.0, jitter_fraction: float = 0.2, max_steps: int | None = None) -> None:
    """Live loop: capture a screenshot, decide an action, execute it,
    repeat. Imports `mss`/`pynput` lazily (same reasoning as
    `cli.py::cmd_record`) so importing this module for `decide_action`
    alone — what the tests do — never requires a display or those
    dependencies installed."""
    import random

    import mss
    import mss.tools
    from pynput.keyboard import Controller as KeyboardController
    from pynput.mouse import Button, Controller as MouseController

    model, key_vocab = load_checkpoint(checkpoint_path)
    model.eval()
    mouse = MouseController()
    keyboard = KeyboardController()

    step = 0
    with mss.mss() as sct:
        monitor = sct.monitors[1]
        while max_steps is None or step < max_steps:
            shot = sct.grab(monitor)
            frame_bytes = mss.tools.to_png(shot.rgb, shot.size)
            temp_path = Path.home() / ".game_agent_rl_play_frame.png"
            temp_path.write_bytes(frame_bytes)
            frame_tensor = load_frame_tensor(temp_path)

            action = decide_action(model, key_vocab, frame_tensor)
            if action["type"] == "click":
                mouse.position = (action["x"], action["y"])
                mouse.click(Button.left)
            elif action["type"] == "key":
                target = decode_key_for_input(action["key"])
                if target is not None:
                    keyboard.press(target)
                    keyboard.release(target)
            # "wait": no input this tick.

            step += 1
            time.sleep(jittered_interval(1.0 / fps, jitter_fraction, random.random()))
