"""Unit tests for play.py's pure functions — decide_action's decoding
logic, the key-name inverse mapping, and the jitter shape. Deliberately
does NOT test `run` (the live capture/input loop): that needs a real
display and would actually move the mouse/press keys on whatever
machine runs the test suite, which is exactly the kind of "looks tested
but isn't really" trap this project's convention avoids (see the
session's established pattern — e.g. the mutex-fix test that was
written then deleted for testing a mock nothing real touches).
Run with: python -m unittest game_agent_rl.test_play
"""

import importlib.util
import unittest

from .train_bc import BehaviorCloningPolicy, KeyVocabulary
from .play import decide_action, decode_key_for_input, jittered_interval

# pynput is real, required tooling for play.py to actually run (same as
# record.py) — but it's dev/research tooling never bundled into the app
# (see requirements.txt), so it isn't guaranteed to be installed in
# every environment these tests run in. Skipping honestly when it's
# absent, rather than failing on a missing optional dependency.
_HAS_PYNPUT = importlib.util.find_spec("pynput") is not None


class DecideActionTests(unittest.TestCase):
    def test_returns_a_well_formed_click_action(self):
        import torch

        vocab = KeyVocabulary(["space", "a"])
        model = BehaviorCloningPolicy(num_keys=vocab.size())
        frame = torch.zeros(1, 84, 84)

        # Force the model's action-type head toward "click" deterministically
        # by zeroing its weights and biasing the click logit, rather than
        # trusting an untrained model's arbitrary argmax — this is testing
        # decide_action's *decoding*, not the model's prediction quality.
        with torch.no_grad():
            model.action_type_head.weight.zero_()
            model.action_type_head.bias.zero_()
            model.action_type_head.bias[0] = 10.0  # ACTION_TYPES[0] == "click"
            model.click_head.weight.zero_()
            model.click_head.bias.zero_()
            model.click_head.bias[0] = 12.7
            model.click_head.bias[1] = 8.4

        action = decide_action(model, vocab, frame)
        self.assertEqual(action, {"type": "click", "x": 13, "y": 8})

    def test_returns_wait_when_a_key_prediction_falls_in_the_unknown_slot(self):
        import torch

        vocab = KeyVocabulary(["space", "a"])
        model = BehaviorCloningPolicy(num_keys=vocab.size())
        frame = torch.zeros(1, 84, 84)

        with torch.no_grad():
            model.action_type_head.weight.zero_()
            model.action_type_head.bias.zero_()
            model.action_type_head.bias[1] = 10.0  # ACTION_TYPES[1] == "key"
            model.key_head.weight.zero_()
            model.key_head.bias.zero_()
            model.key_head.bias[-1] = 10.0  # the reserved "unknown" slot

        action = decide_action(model, vocab, frame)
        self.assertEqual(action, {"type": "wait"})

    def test_returns_a_recognized_key_action(self):
        import torch

        vocab = KeyVocabulary(["space", "a"])
        model = BehaviorCloningPolicy(num_keys=vocab.size())
        frame = torch.zeros(1, 84, 84)

        with torch.no_grad():
            model.action_type_head.weight.zero_()
            model.action_type_head.bias.zero_()
            model.action_type_head.bias[1] = 10.0  # ACTION_TYPES[1] == "key"
            model.key_head.weight.zero_()
            model.key_head.bias.zero_()
            model.key_head.bias[0] = 10.0  # vocab.keys[0] == "a" (sorted)

        action = decide_action(model, vocab, frame)
        self.assertEqual(action, {"type": "key", "key": "a"})


class DecodeKeyForInputTests(unittest.TestCase):
    def test_decodes_a_plain_character_key(self):
        # Doesn't need pynput at all — "'a'" never takes the Key. branch.
        self.assertEqual(decode_key_for_input("'a'"), "a")

    @unittest.skipUnless(_HAS_PYNPUT, "pynput not installed")
    def test_decodes_a_special_key_name(self):
        from pynput.keyboard import Key

        self.assertEqual(decode_key_for_input("Key.space"), Key.space)

    @unittest.skipUnless(_HAS_PYNPUT, "pynput not installed")
    def test_an_unrecognized_special_key_name_decodes_to_none_rather_than_raising(self):
        self.assertIsNone(decode_key_for_input("Key.not_a_real_key"))


class JitteredIntervalTests(unittest.TestCase):
    def test_zero_jitter_returns_the_base_interval_exactly(self):
        self.assertEqual(jittered_interval(1.0, 0.0, 0.5), 1.0)

    def test_stays_within_the_declared_jitter_range(self):
        for random_unit in (0.0, 0.25, 0.5, 0.75, 1.0):
            value = jittered_interval(1.0, 0.2, random_unit)
            self.assertGreaterEqual(value, 0.8)
            self.assertLessEqual(value, 1.2)

    def test_never_returns_a_negative_interval_for_out_of_range_jitter(self):
        self.assertGreaterEqual(jittered_interval(1.0, 5.0, 0.0), 0.0)

    def test_clamps_an_out_of_range_random_unit_rather_than_extrapolating(self):
        self.assertEqual(jittered_interval(1.0, 0.2, 2.0), jittered_interval(1.0, 0.2, 1.0))
        self.assertEqual(jittered_interval(1.0, 0.2, -1.0), jittered_interval(1.0, 0.2, 0.0))


if __name__ == "__main__":
    unittest.main()
