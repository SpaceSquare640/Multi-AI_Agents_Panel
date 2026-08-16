"""Unit tests for the pure bookkeeping in record.py — no real screen or
input device needed. Run with: python -m unittest game_agent_rl.test_record
"""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from .record import Recorder, frame_filename, serialize_event, session_dir


class SessionDirTests(unittest.TestCase):
    def test_session_dir_joins_output_dir_and_session_name(self):
        self.assertEqual(session_dir("/tmp/out", "run1"), Path("/tmp/out/run1"))


class FrameFilenameTests(unittest.TestCase):
    def test_frame_filename_is_zero_padded_for_stable_sort_order(self):
        self.assertEqual(frame_filename(0), "frame_000000.png")
        self.assertEqual(frame_filename(42), "frame_000042.png")

    def test_frame_filenames_sort_correctly_as_plain_strings(self):
        names = sorted(frame_filename(i) for i in [2, 10, 1, 100])
        self.assertEqual(names, [frame_filename(1), frame_filename(2), frame_filename(10), frame_filename(100)])


class SerializeEventTests(unittest.TestCase):
    def test_serialize_event_round_trips_through_json(self):
        line = serialize_event("click", 1.5, x=10, y=20, button="Button.left", pressed=True)
        parsed = json.loads(line)
        self.assertEqual(parsed, {"type": "click", "t": 1.5, "x": 10, "y": 20, "button": "Button.left", "pressed": True})


class RecorderTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)
        self.captured_paths = []

    def fake_capture(self, path: Path) -> None:
        # Doesn't touch the real screen — just records that it was
        # asked to capture, and writes a placeholder file so callers
        # that check for the file's existence still see one.
        self.captured_paths.append(path)
        path.write_bytes(b"")

    def test_creates_the_session_directory(self):
        recorder = Recorder(self.tmp_dir, "session-a", self.fake_capture)
        self.assertTrue(recorder.dir.is_dir())
        self.assertEqual(recorder.dir, self.tmp_dir / "session-a")

    def test_capture_one_frame_increments_frame_count_and_uses_the_injected_capture_function(self):
        recorder = Recorder(self.tmp_dir, "session-b", self.fake_capture)
        first = recorder.capture_one_frame()
        second = recorder.capture_one_frame()
        self.assertEqual(recorder.frame_count, 2)
        self.assertEqual(first.name, "frame_000000.png")
        self.assertEqual(second.name, "frame_000001.png")
        self.assertEqual(self.captured_paths, [first, second])

    def test_log_event_appends_one_json_line_per_call(self):
        recorder = Recorder(self.tmp_dir, "session-c", self.fake_capture)
        recorder.log_event("click", x=1, y=2, button="left", pressed=True)
        recorder.log_event("key", key="'a'")

        lines = (recorder.dir / "events.jsonl").read_text(encoding="utf-8").strip().splitlines()
        self.assertEqual(len(lines), 2)
        first = json.loads(lines[0])
        second = json.loads(lines[1])
        self.assertEqual(first["type"], "click")
        self.assertEqual(second["type"], "key")
        # Elapsed time should be non-negative and monotonically
        # non-decreasing across calls, not a fixed/faked value.
        self.assertGreaterEqual(first["t"], 0)
        self.assertGreaterEqual(second["t"], first["t"])


if __name__ == "__main__":
    unittest.main()
