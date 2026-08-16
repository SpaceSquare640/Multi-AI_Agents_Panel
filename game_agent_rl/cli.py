"""CLI entry point for Track B's offline tooling (see
Game-Playing Agent Design.md section 4). Only `record` is implemented so
far — `label`/`train-bc`/`train-rl`/`play` are future subcommands, not
stubbed out with fake behavior (this project's convention: don't pretend
something works when it doesn't exist yet).

Usage: python -m game_agent_rl.cli record --session <name> --output-dir <dir>
"""

import argparse
import sys
import time
from pathlib import Path

from .record import Recorder


def _default_capture_frame(path: Path) -> None:
    """Real screen capture — imports `mss` lazily so `record.py`'s pure
    logic (and its tests) don't need the dependency installed just to
    exercise the bookkeeping."""
    import mss
    import mss.tools

    with mss.mss() as sct:
        monitor = sct.monitors[1]
        shot = sct.grab(monitor)
        mss.tools.to_png(shot.rgb, shot.size, output=str(path))


def cmd_record(args: argparse.Namespace) -> None:
    from pynput import keyboard, mouse

    recorder = Recorder(Path(args.output_dir), args.session, _default_capture_frame)
    print(f"Recording to {recorder.dir} — Ctrl+C to stop.", file=sys.stderr)

    def on_click(x, y, button, pressed):
        recorder.log_event("click", x=x, y=y, button=str(button), pressed=pressed)

    def on_key_press(key):
        recorder.log_event("key", key=str(key))

    mouse_listener = mouse.Listener(on_click=on_click)
    keyboard_listener = keyboard.Listener(on_press=on_key_press)
    mouse_listener.start()
    keyboard_listener.start()

    try:
        while True:
            recorder.capture_one_frame()
            time.sleep(1.0 / args.fps)
    except KeyboardInterrupt:
        pass
    finally:
        mouse_listener.stop()
        keyboard_listener.stop()
        print(f"Stopped. {recorder.frame_count} frames recorded.", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(prog="game_agent_rl")
    sub = parser.add_subparsers(dest="command", required=True)

    record_parser = sub.add_parser("record", help="Record a human demonstration session")
    record_parser.add_argument("--session", required=True, help="Session name (used as the output folder name)")
    record_parser.add_argument("--output-dir", required=True, help="Directory to write session folders into")
    record_parser.add_argument("--fps", type=float, default=2.0, help="Screenshot frames per second (default: 2)")
    record_parser.set_defaults(func=cmd_record)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
