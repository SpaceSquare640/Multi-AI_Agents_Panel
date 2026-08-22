"""Unit tests for label.py — synthetic events only, no real recorded
session needed (see label.py's module docstring for why that's an
honest scope for this stage). Run with:
python -m unittest game_agent_rl.test_label
"""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from .label import label_frames, label_session, read_events


def frame_event(index: int, t: float) -> dict:
    return {"type": "frame", "t": t, "index": index, "filename": f"frame_{index:06d}.png"}


def click_event(t: float, x: int, y: int) -> dict:
    return {"type": "click", "t": t, "x": x, "y": y, "button": "Button.left", "pressed": True}


def key_event(t: float, key: str) -> dict:
    return {"type": "key", "t": t, "key": key}


class LabelFramesTests(unittest.TestCase):
    def test_labels_a_frame_with_a_click_that_follows_it_within_the_window(self):
        events = [frame_event(0, 0.0), click_event(0.4, 100, 200)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels, [{"frame": "frame_000000.png", "action": {"type": "click", "x": 100, "y": 200}}])

    def test_labels_a_frame_with_a_key_press_that_follows_it(self):
        events = [frame_event(0, 0.0), key_event(0.2, "space")]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels[0]["action"], {"type": "key", "key": "space"})

    def test_labels_a_frame_as_wait_when_no_action_follows_within_the_window(self):
        events = [frame_event(0, 0.0), click_event(2.0, 1, 1)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels, [{"frame": "frame_000000.png", "action": {"type": "wait"}}])

    def test_labels_a_frame_as_wait_when_there_are_no_actions_at_all(self):
        events = [frame_event(0, 0.0), frame_event(1, 0.5)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual([l["action"] for l in labels], [{"type": "wait"}, {"type": "wait"}])

    def test_picks_the_earliest_qualifying_action_when_several_are_within_the_window(self):
        events = [frame_event(0, 0.0), click_event(0.8, 9, 9), click_event(0.2, 1, 2)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels[0]["action"], {"type": "click", "x": 1, "y": 2})

    def test_an_action_before_a_frames_timestamp_does_not_label_it(self):
        # An action that happened *before* the frame was captured is the
        # human reacting to an *earlier* frame, not this one.
        events = [frame_event(0, 1.0), click_event(0.5, 3, 4)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels[0]["action"], {"type": "wait"})

    def test_a_shared_close_action_can_label_more_than_one_frame(self):
        events = [frame_event(0, 0.0), frame_event(1, 0.3), click_event(0.5, 5, 5)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels[0]["action"], {"type": "click", "x": 5, "y": 5})
        self.assertEqual(labels[1]["action"], {"type": "click", "x": 5, "y": 5})

    def test_sorts_out_of_order_events_before_matching(self):
        events = [click_event(0.4, 1, 1), frame_event(0, 0.0)]
        labels = label_frames(events, window_seconds=1.0)
        self.assertEqual(labels[0]["action"], {"type": "click", "x": 1, "y": 1})

    def test_produces_no_labels_when_there_are_no_frames(self):
        events = [click_event(0.0, 1, 1)]
        self.assertEqual(label_frames(events), [])


class ReadEventsTests(unittest.TestCase):
    def test_reads_jsonl_in_file_order_skipping_blank_lines(self):
        tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp_dir, ignore_errors=True)
        events_path = tmp_dir / "events.jsonl"
        events_path.write_text('{"type":"frame","t":0.0,"index":0,"filename":"frame_000000.png"}\n\n{"type":"click","t":0.1,"x":1,"y":2}\n')
        events = read_events(events_path)
        self.assertEqual(len(events), 2)
        self.assertEqual(events[0]["type"], "frame")
        self.assertEqual(events[1]["type"], "click")


class LabelSessionTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)

    def test_writes_labels_jsonl_and_returns_the_same_labels(self):
        events_path = self.tmp_dir / "events.jsonl"
        with open(events_path, "w", encoding="utf-8") as f:
            f.write(json.dumps(frame_event(0, 0.0)) + "\n")
            f.write(json.dumps(click_event(0.2, 7, 8)) + "\n")

        labels = label_session(self.tmp_dir)
        self.assertEqual(labels, [{"frame": "frame_000000.png", "action": {"type": "click", "x": 7, "y": 8}}])

        written = (self.tmp_dir / "labels.jsonl").read_text(encoding="utf-8").strip().splitlines()
        self.assertEqual(len(written), 1)
        self.assertEqual(json.loads(written[0]), labels[0])


if __name__ == "__main__":
    unittest.main()
