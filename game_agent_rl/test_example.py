"""Unit tests for example.py. Run with:
python -m unittest game_agent_rl.test_example
"""

import shutil
import tempfile
import unittest
from pathlib import Path

from .example import generate_example_session
from .label import label_session


class GenerateExampleSessionTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)

    def test_writes_a_frame_per_declared_action_and_an_events_file(self):
        session_path = generate_example_session(self.tmp_dir)
        frames = sorted(session_path.glob("frame_*.png"))
        self.assertEqual(len(frames), 6)
        self.assertTrue((session_path / "events.jsonl").exists())

    def test_every_frame_is_a_real_openable_png(self):
        from PIL import Image

        session_path = generate_example_session(self.tmp_dir)
        for frame in sorted(session_path.glob("frame_*.png")):
            with Image.open(frame) as image:
                image.verify()

    def test_is_deterministic_across_runs(self):
        first = generate_example_session(self.tmp_dir, session_name="run-a")
        second = generate_example_session(self.tmp_dir, session_name="run-b")
        first_bytes = (first / "frame_000000.png").read_bytes()
        second_bytes = (second / "frame_000000.png").read_bytes()
        self.assertEqual(first_bytes, second_bytes)
        self.assertEqual((first / "events.jsonl").read_text(), (second / "events.jsonl").read_text())

    def test_the_generated_session_produces_a_real_mix_of_labels_through_the_real_pipeline(self):
        # The point of the example session: it should exercise every
        # action type the rest of the pipeline understands, verified by
        # actually running label.py's real label_session against it —
        # not just asserting on example.py's own output in isolation.
        session_path = generate_example_session(self.tmp_dir)
        labels = label_session(session_path)

        action_types = {label["action"]["type"] for label in labels}
        self.assertEqual(action_types, {"click", "key", "wait"})
        self.assertEqual(len(labels), 6)


if __name__ == "__main__":
    unittest.main()
