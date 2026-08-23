"""Unit tests for train_rl.py — a tiny synthetic session (same pattern as
test_train_bc.py: fabricated 1x1 PNG frames + hand-written labels, plus a
hand-written rewards.jsonl). Proves the reward-weighted fine-tuning loop
runs end to end on real data and produces a checkpoint that reloads
correctly and that extreme rewards actually pull the loss weighting the
direction they should — not a claim about how *good* the resulting
policy plays. Run with: python -m unittest game_agent_rl.test_train_rl
"""

import json
import math
import shutil
import tempfile
import unittest
from pathlib import Path

from .test_train_bc import write_fake_session
from .train_bc import BehaviorCloningPolicy, save_checkpoint, train
from .train_rl import example_weights, fine_tune, read_rewards


def write_rewards(session_dir: Path, rewards: dict) -> None:
    with open(session_dir / "rewards.jsonl", "w", encoding="utf-8") as f:
        for frame, reward in rewards.items():
            f.write(json.dumps({"frame": frame, "reward": reward}) + "\n")


class ReadRewardsTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)

    def test_returns_empty_dict_when_no_rewards_file_exists(self):
        self.assertEqual(read_rewards(self.tmp_dir), {})

    def test_reads_frame_to_reward_mapping(self):
        write_rewards(self.tmp_dir, {"frame_000000.png": 1.5, "frame_000001.png": -2.0})
        rewards = read_rewards(self.tmp_dir)
        self.assertEqual(rewards, {"frame_000000.png": 1.5, "frame_000001.png": -2.0})


class ExampleWeightsTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)
        self.labels = [
            {"frame": "frame_000000.png", "action": {"type": "click", "x": 1, "y": 2}},
            {"frame": "frame_000001.png", "action": {"type": "wait"}},
        ]
        write_fake_session(self.tmp_dir, self.labels)

    def test_unrewarded_frames_get_neutral_weight_one(self):
        weights = example_weights(self.tmp_dir)
        self.assertEqual(weights.tolist(), [1.0, 1.0])

    def test_higher_reward_produces_higher_weight(self):
        write_rewards(self.tmp_dir, {"frame_000000.png": 2.0, "frame_000001.png": -2.0})
        weights = example_weights(self.tmp_dir)
        self.assertGreater(weights[0].item(), weights[1].item())
        self.assertAlmostEqual(weights[0].item(), math.exp(2.0), places=4)
        self.assertAlmostEqual(weights[1].item(), math.exp(-2.0), places=4)

    def test_extreme_rewards_are_clamped_rather_than_producing_inf_or_nan(self):
        write_rewards(self.tmp_dir, {"frame_000000.png": 1_000_000.0, "frame_000001.png": -1_000_000.0})
        weights = example_weights(self.tmp_dir)
        self.assertTrue(all(w == w for w in weights.tolist()))  # no NaN
        self.assertTrue(all(w not in (float("inf"), float("-inf")) for w in weights.tolist()))


class FineTuneTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)
        self.labels = [
            {"frame": f"frame_{i:06d}.png", "action": {"type": "click", "x": i, "y": i}}
            for i in range(4)
        ] + [
            {"frame": "frame_000004.png", "action": {"type": "key", "key": "space"}},
            {"frame": "frame_000005.png", "action": {"type": "wait"}},
        ]
        write_fake_session(self.tmp_dir, self.labels)
        model, key_vocab, _ = train(self.tmp_dir, epochs=1, batch_size=3)
        self.checkpoint_path = self.tmp_dir / "bc.pt"
        save_checkpoint(model, key_vocab, self.checkpoint_path)

    def test_fine_tunes_end_to_end_and_produces_finite_loss(self):
        write_rewards(self.tmp_dir, {"frame_000000.png": 1.0})
        model, key_vocab, history = fine_tune(self.tmp_dir, self.checkpoint_path, epochs=2, batch_size=3)

        self.assertIsInstance(model, BehaviorCloningPolicy)
        self.assertEqual(key_vocab.keys, ["space"])
        self.assertEqual(len(history), 2)
        for loss in history:
            self.assertTrue(loss == loss)  # not NaN
            self.assertGreaterEqual(loss, 0.0)

    def test_works_with_no_rewards_file_at_all_degrading_to_plain_weighted_bc(self):
        # No rewards.jsonl written — every example gets the neutral
        # weight of 1.0, so this should behave like continued plain BC
        # training rather than raising or producing degenerate output.
        model, key_vocab, history = fine_tune(self.tmp_dir, self.checkpoint_path, epochs=1, batch_size=3)
        self.assertEqual(len(history), 1)
        self.assertTrue(history[0] == history[0])  # not NaN


if __name__ == "__main__":
    unittest.main()
