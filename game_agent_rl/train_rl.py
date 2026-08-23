"""Track B (Deep RL) — train-rl stage. See the Game-Playing Agent design
notes, section 4, step 4: fine-tunes a behavior-cloning checkpoint
(`train_bc.py`) using per-frame rewards, so the policy shifts toward the
higher-reward actions in a session rather than imitating every recorded
action equally.

**Why this shape and not online PPO against a live game**: this repo has
no game environment abstraction — Track B trains from recorded frames,
it doesn't drive a live game loop during training (that's `play.py`'s
job, after training is done). A "true" online RL algorithm (PPO, DQN,
...) needs an environment it can step and get a live reward signal
from, which is inherently game-specific — nothing generic to build here
that would actually work against an arbitrary game. What *is* generic
and genuinely implementable is offline/batch RL fine-tuning from a
reward-annotated dataset: this uses reward-weighted regression (a real,
established technique — see Peters & Schaal 2007, and Advantage-Weighted
Regression / Nair et al. 2020 for the modern form), which continues
training the BC checkpoint's existing loss but scales each example's
contribution by `exp(reward / temperature)` instead of weighting every
example equally. High-reward actions pull the policy toward them harder
than low- or negative-reward ones; it degrades gracefully to plain BC
when every reward is equal.

Rewards are supplied by the user as `rewards.jsonl` next to a session's
`labels.jsonl` — one `{"frame": "<name>.png", "reward": <float>}` per
labeled frame. There is no automatic reward signal (score-reading,
win/loss detection, ...) — that would be game-specific UI parsing this
project has no way to build generically, and pretending otherwise would
be exactly the kind of unearned "it just works" claim this project's
Track B has deliberately avoided since `train_bc.py`.
"""

import json
import math
from pathlib import Path

import torch
from torch.utils.data import DataLoader, Dataset

from .train_bc import (
    ACTION_TYPES,
    DemonstrationDataset,
    load_checkpoint,
    read_labels,
    save_checkpoint,
)


def read_rewards(session_dir) -> dict:
    """Returns `{frame_filename: reward}` from `rewards.jsonl` next to a
    session's `labels.jsonl`. A frame with no entry gets reward `0.0`
    (neutral — neither reinforced nor suppressed) rather than raising,
    since a user may only have bothered to annotate the frames they
    actually cared about."""
    rewards_path = Path(session_dir) / "rewards.jsonl"
    if not rewards_path.exists():
        return {}
    rewards = {}
    for line in rewards_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        entry = json.loads(line)
        rewards[entry["frame"]] = float(entry["reward"])
    return rewards


def example_weights(session_dir, temperature: float = 1.0) -> torch.Tensor:
    """One `exp(reward / temperature)` weight per labeled frame, in the
    same order `DemonstrationDataset` iterates them — the reward-weighted
    regression update. Clamped to a sane range so one extreme reward
    can't produce an inf/NaN weight and silently poison the whole batch's
    gradient."""
    labels = read_labels(session_dir)
    rewards = read_rewards(session_dir)
    weights = [math.exp(max(min(rewards.get(label["frame"], 0.0) / temperature, 20.0), -20.0)) for label in labels]
    return torch.tensor(weights, dtype=torch.float32)


class _WeightedDemonstrationDataset(Dataset):
    """Wraps `DemonstrationDataset` so each example carries its reward
    weight alongside it — letting a single shuffled `DataLoader` keep
    frame/action/weight correctly paired per example, rather than
    trying to recover a shuffled batch's original indices after the
    fact (which a naive parallel unshuffled loader gets wrong: its
    batches line up with a *different* shuffle than the data loader's,
    silently pairing each example with the wrong weight)."""

    def __init__(self, session_dir, key_vocab, temperature: float):
        self.inner = DemonstrationDataset(session_dir, key_vocab)
        self.weights = example_weights(session_dir, temperature)

    def __len__(self) -> int:
        return len(self.inner)

    def __getitem__(self, idx: int):
        frame, action_type, click_xy, key_index = self.inner[idx]
        return frame, action_type, click_xy, key_index, self.weights[idx]


def fine_tune(session_dir, checkpoint_in, epochs: int = 1, batch_size: int = 8, lr: float = 1e-4, temperature: float = 1.0) -> tuple:
    """Loads `checkpoint_in`, continues training it on `session_dir` with
    each example's loss scaled by its reward weight, and returns
    `(model, key_vocab, loss_history)` — real training output, same
    honesty contract as `train_bc.train`: this proves the fine-tuning
    loop runs end to end and the loss is finite, not a claim about how
    good the resulting policy is."""
    model, key_vocab = load_checkpoint(checkpoint_in)
    dataset = _WeightedDemonstrationDataset(session_dir, key_vocab, temperature)
    loader = DataLoader(dataset, batch_size=min(batch_size, len(dataset)), shuffle=True)

    optimizer = torch.optim.Adam(model.parameters(), lr=lr)
    action_type_loss_fn = torch.nn.CrossEntropyLoss(reduction="none")
    click_loss_fn = torch.nn.MSELoss(reduction="none")
    key_loss_fn = torch.nn.CrossEntropyLoss(ignore_index=-1, reduction="none")

    history = []
    for _ in range(epochs):
        total_loss = 0.0
        batches = 0
        for frame, action_type, click_xy, key_index, batch_weights in loader:
            optimizer.zero_grad()
            type_logits, click_pred, key_logits = model(frame)

            per_example = action_type_loss_fn(type_logits, action_type)

            click_mask = action_type == ACTION_TYPES.index("click")
            if click_mask.any():
                click_losses = click_loss_fn(click_pred, click_xy).mean(dim=1)
                per_example = per_example + torch.where(click_mask, click_losses, torch.zeros_like(click_losses))

            key_mask = key_index != -1
            if key_mask.any():
                key_losses = key_loss_fn(key_logits, key_index)
                per_example = per_example + torch.where(key_mask, key_losses, torch.zeros_like(key_losses))

            loss = (per_example * batch_weights).sum() / batch_weights.sum().clamp(min=1e-8)
            loss.backward()
            optimizer.step()
            total_loss += loss.item()
            batches += 1
        history.append(total_loss / max(batches, 1))

    return model, key_vocab, history


__all__ = ["fine_tune", "read_rewards", "example_weights", "save_checkpoint"]
