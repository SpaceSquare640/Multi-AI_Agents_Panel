"""Unit tests for train_bc.py — a tiny synthetic session (a handful of
1x1 fabricated PNG frames + hand-written labels), not a real recorded
demonstration. This proves the training loop actually runs end to end
and produces a checkpoint that reloads correctly — it does NOT prove
the resulting policy plays anything well, which needs real
demonstration data this repo doesn't have (see train_bc.py's module
docstring). Run with: python -m unittest game_agent_rl.test_train_bc
"""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from PIL import Image

from .train_bc import (
    ACTION_TYPES,
    FRAME_SIZE,
    BehaviorCloningPolicy,
    DemonstrationDataset,
    KeyVocabulary,
    action_type_index,
    build_key_vocabulary,
    load_checkpoint,
    load_frame_tensor,
    save_checkpoint,
    train,
)


def write_fake_session(session_dir: Path, labels: list) -> None:
    session_dir.mkdir(parents=True, exist_ok=True)
    for label in labels:
        # A real frame is a screenshot; a solid-color 1x1 PNG is enough
        # to exercise the real image-loading/resize/tensor-conversion
        # path without needing an actual screen capture.
        Image.new("RGB", (1, 1), color=(255, 0, 0)).save(session_dir / label["frame"])
    with open(session_dir / "labels.jsonl", "w", encoding="utf-8") as f:
        for label in labels:
            f.write(json.dumps(label) + "\n")


class ActionTypeIndexTests(unittest.TestCase):
    def test_matches_the_declared_action_types_order(self):
        self.assertEqual(action_type_index({"type": "click"}), ACTION_TYPES.index("click"))
        self.assertEqual(action_type_index({"type": "wait"}), ACTION_TYPES.index("wait"))


class LoadFrameTensorTests(unittest.TestCase):
    def test_produces_a_correctly_shaped_grayscale_tensor_in_zero_one_range(self):
        tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp_dir, ignore_errors=True)
        frame_path = tmp_dir / "frame.png"
        Image.new("RGB", (10, 10), color=(128, 128, 128)).save(frame_path)

        tensor = load_frame_tensor(frame_path)
        self.assertEqual(tuple(tensor.shape), (1, FRAME_SIZE, FRAME_SIZE))
        self.assertTrue((tensor >= 0).all())
        self.assertTrue((tensor <= 1).all())


class KeyVocabularyTests(unittest.TestCase):
    def test_assigns_stable_sorted_indices(self):
        vocab = KeyVocabulary(["space", "a", "space"])
        self.assertEqual(vocab.keys, ["a", "space"])
        self.assertEqual(vocab.index_of("a"), 0)
        self.assertEqual(vocab.index_of("space"), 1)

    def test_an_unseen_key_maps_to_the_reserved_unknown_slot(self):
        vocab = KeyVocabulary(["a", "b"])
        self.assertEqual(vocab.index_of("never seen"), 2)
        self.assertEqual(vocab.size(), 3)

    def test_round_trips_through_to_dict_from_dict(self):
        vocab = KeyVocabulary(["b", "a"])
        restored = KeyVocabulary.from_dict(vocab.to_dict())
        self.assertEqual(restored.keys, vocab.keys)
        self.assertEqual(restored.index_of("a"), vocab.index_of("a"))


class BuildKeyVocabularyTests(unittest.TestCase):
    def test_only_collects_keys_from_key_actions(self):
        labels = [
            {"action": {"type": "key", "key": "space"}},
            {"action": {"type": "click", "x": 1, "y": 2}},
            {"action": {"type": "wait"}},
        ]
        vocab = build_key_vocabulary(labels)
        self.assertEqual(vocab.keys, ["space"])


class DemonstrationDatasetTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)

    def test_returns_one_example_per_labeled_frame_with_correct_shapes(self):
        labels = [
            {"frame": "frame_000000.png", "action": {"type": "click", "x": 5, "y": 9}},
            {"frame": "frame_000001.png", "action": {"type": "key", "key": "space"}},
            {"frame": "frame_000002.png", "action": {"type": "wait"}},
        ]
        write_fake_session(self.tmp_dir, labels)
        vocab = build_key_vocabulary(labels)
        dataset = DemonstrationDataset(self.tmp_dir, vocab)

        self.assertEqual(len(dataset), 3)
        frame, action_type, click_xy, key_index = dataset[0]
        self.assertEqual(tuple(frame.shape), (1, FRAME_SIZE, FRAME_SIZE))
        self.assertEqual(action_type, ACTION_TYPES.index("click"))
        self.assertEqual(click_xy.tolist(), [5.0, 9.0])
        self.assertEqual(key_index, -1)

        _, key_action_type, _, key_index = dataset[1]
        self.assertEqual(key_action_type, ACTION_TYPES.index("key"))
        self.assertEqual(key_index, vocab.index_of("space"))


class BehaviorCloningPolicyTests(unittest.TestCase):
    def test_forward_pass_produces_correctly_shaped_outputs(self):
        import torch

        model = BehaviorCloningPolicy(num_keys=4)
        batch = torch.zeros(2, 1, FRAME_SIZE, FRAME_SIZE)
        type_logits, click_pred, key_logits = model(batch)
        self.assertEqual(tuple(type_logits.shape), (2, len(ACTION_TYPES)))
        self.assertEqual(tuple(click_pred.shape), (2, 2))
        self.assertEqual(tuple(key_logits.shape), (2, 4))


class TrainTests(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp_dir, ignore_errors=True)

    def test_trains_end_to_end_on_a_tiny_synthetic_session_and_produces_finite_loss(self):
        labels = [
            {"frame": f"frame_{i:06d}.png", "action": {"type": "click", "x": i, "y": i}}
            for i in range(4)
        ] + [
            {"frame": "frame_000004.png", "action": {"type": "key", "key": "space"}},
            {"frame": "frame_000005.png", "action": {"type": "wait"}},
        ]
        write_fake_session(self.tmp_dir, labels)

        model, key_vocab, history = train(self.tmp_dir, epochs=2, batch_size=3)

        self.assertIsInstance(model, BehaviorCloningPolicy)
        self.assertEqual(key_vocab.keys, ["space"])
        self.assertEqual(len(history), 2)
        for loss in history:
            self.assertTrue(loss == loss)  # not NaN
            self.assertGreaterEqual(loss, 0.0)

    def test_checkpoint_round_trips_and_the_reloaded_model_matches_the_original(self):
        import torch

        labels = [
            {"frame": "frame_000000.png", "action": {"type": "click", "x": 1, "y": 2}},
            {"frame": "frame_000001.png", "action": {"type": "key", "key": "a"}},
        ]
        write_fake_session(self.tmp_dir, labels)
        model, key_vocab, _ = train(self.tmp_dir, epochs=1, batch_size=2)

        checkpoint_path = self.tmp_dir / "policy.pt"
        save_checkpoint(model, key_vocab, checkpoint_path)
        self.assertTrue(checkpoint_path.exists())

        reloaded_model, reloaded_vocab = load_checkpoint(checkpoint_path)
        self.assertEqual(reloaded_vocab.keys, key_vocab.keys)

        original_params = list(model.state_dict().values())
        reloaded_params = list(reloaded_model.state_dict().values())
        self.assertEqual(len(original_params), len(reloaded_params))
        for original, reloaded in zip(original_params, reloaded_params):
            self.assertTrue(torch.equal(original, reloaded))


if __name__ == "__main__":
    unittest.main()
