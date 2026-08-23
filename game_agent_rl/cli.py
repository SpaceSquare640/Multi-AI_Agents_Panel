"""CLI entry point for Track B's offline tooling (see
Game-Playing Agent Design.md section 4). `record`/`label`/`train-bc` are
implemented — `train-rl`/`play` are future subcommands, not stubbed out
with fake behavior (this project's convention: don't pretend something
works when it doesn't exist yet).

Usage:
  python -m game_agent_rl.cli record --session <name> --output-dir <dir>
  python -m game_agent_rl.cli label --session-dir <dir> [--window-seconds <n>]
  python -m game_agent_rl.cli train-bc --session-dir <dir> --checkpoint-out <path> [--epochs <n>]
  python -m game_agent_rl.cli example --output-dir <dir> [--session <name>]
"""

import argparse
import sys
import time
from pathlib import Path

from .example import generate_example_session
from .label import label_session
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


def cmd_label(args: argparse.Namespace) -> None:
    labels = label_session(Path(args.session_dir), window_seconds=args.window_seconds)
    waits = sum(1 for label in labels if label["action"]["type"] == "wait")
    print(
        f"Labeled {len(labels)} frames ({len(labels) - waits} with an action, {waits} as wait) "
        f"-> {Path(args.session_dir) / 'labels.jsonl'}",
        file=sys.stderr,
    )


def cmd_train_bc(args: argparse.Namespace) -> None:
    # Lazy import — `torch`/`Pillow` are only needed for this
    # subcommand, same reasoning as `mss`/`pynput` being imported inside
    # `cmd_record` rather than at module load time: `record`/`label`
    # shouldn't require installing torch just to run.
    from .train_bc import save_checkpoint, train

    model, key_vocab, history = train(Path(args.session_dir), epochs=args.epochs, batch_size=args.batch_size)
    save_checkpoint(model, key_vocab, Path(args.checkpoint_out))
    loss_trace = " -> ".join(f"{loss:.4f}" for loss in history)
    print(
        f"Trained {args.epochs} epoch(s) on {args.session_dir} "
        f"(loss per epoch: {loss_trace}) -> {args.checkpoint_out}",
        file=sys.stderr,
    )


def cmd_example(args: argparse.Namespace) -> None:
    session_path = generate_example_session(Path(args.output_dir), args.session)
    print(
        f"Wrote a synthetic (non-real) example session to {session_path} — "
        f"try: python -m game_agent_rl.cli label --session-dir {session_path}",
        file=sys.stderr,
    )


def main() -> None:
    parser = argparse.ArgumentParser(prog="game_agent_rl")
    sub = parser.add_subparsers(dest="command", required=True)

    record_parser = sub.add_parser("record", help="Record a human demonstration session")
    record_parser.add_argument("--session", required=True, help="Session name (used as the output folder name)")
    record_parser.add_argument("--output-dir", required=True, help="Directory to write session folders into")
    record_parser.add_argument("--fps", type=float, default=2.0, help="Screenshot frames per second (default: 2)")
    record_parser.set_defaults(func=cmd_record)

    label_parser = sub.add_parser("label", help="Label a recorded session's frames with (frame, action) pairs")
    label_parser.add_argument("--session-dir", required=True, help="A session folder produced by `record` (contains events.jsonl)")
    label_parser.add_argument(
        "--window-seconds",
        type=float,
        default=1.0,
        help="How long after a frame an input event can still count as its label (default: 1.0)",
    )
    label_parser.set_defaults(func=cmd_label)

    train_bc_parser = sub.add_parser("train-bc", help="Train a behavior-cloning policy from a labeled session")
    train_bc_parser.add_argument("--session-dir", required=True, help="A session folder produced by `label` (contains labels.jsonl)")
    train_bc_parser.add_argument("--checkpoint-out", required=True, help="Where to write the trained policy checkpoint")
    train_bc_parser.add_argument("--epochs", type=int, default=10, help="Training epochs (default: 10)")
    train_bc_parser.add_argument("--batch-size", type=int, default=8, help="Training batch size (default: 8)")
    train_bc_parser.set_defaults(func=cmd_train_bc)

    example_parser = sub.add_parser(
        "example", help="Generate a small synthetic (non-real) example session to try the pipeline against"
    )
    example_parser.add_argument("--output-dir", required=True, help="Directory to write the example session folder into")
    example_parser.add_argument("--session", default="example", help="Session name (used as the output folder name, default: example)")
    example_parser.set_defaults(func=cmd_example)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
