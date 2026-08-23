"""Track B (Deep RL) — train-bc stage (behavior cloning). See
`Multi-AI Agent Panel Document/04 Agents & Orchestration/Game-Playing Agent Design.md`
section 4, step 3: trains a small policy network to imitate the human
demonstration a session's `labels.jsonl` (see `label.py`) captured, as a
starting point for later RL fine-tuning rather than training from random
play.

**Scope, per the vault's explicit clarification (`2026-08-21`,
"Track B 的交付標準是「框架」，不是「開箱即玩」")**: this is a real,
runnable pipeline stage — not a stub — but it ships no pretrained
weights or demonstration data. It trains on whatever session directory
(frames + `labels.jsonl`) the user points it at. The network here is
deliberately small and unturned (a handful of conv layers over a
downsized grayscale frame) — proving the training loop actually runs
end to end on real data and produces a checkpoint that reloads
correctly, not a tuned architecture claiming good play quality. No
demonstration data exists in this repo to train a *good* policy from
(see the Daily Log's 待釐清 history) — that's an honest limitation of
the project's current state, not of this code.

Avoids `torchvision` (not already a dependency here) — the only thing
needed from it is resize + grayscale + tensor conversion, which PIL and
plain `torch` cover directly.
"""

import json
from pathlib import Path

import torch
from PIL import Image
from torch import nn
from torch.utils.data import DataLoader, Dataset

# Small, Atari-DQN-style frame size — chosen so this trains on CPU in a
# reasonable time as a framework/skeleton, not for image fidelity.
FRAME_SIZE = 84

ACTION_TYPES = ["click", "key", "wait"]


def action_type_index(action: dict) -> int:
    return ACTION_TYPES.index(action["type"])


def load_frame_tensor(path) -> torch.Tensor:
    """Loads a PNG frame, resizes to `FRAME_SIZE`x`FRAME_SIZE`, converts
    to grayscale, and returns a `(1, FRAME_SIZE, FRAME_SIZE)` float
    tensor in `[0, 1]` — the plain-PIL equivalent of the
    `torchvision.transforms` pipeline this module deliberately avoids
    (see module docstring)."""
    image = Image.open(path).convert("L").resize((FRAME_SIZE, FRAME_SIZE))
    tensor = torch.frombuffer(bytearray(image.tobytes()), dtype=torch.uint8).float() / 255.0
    return tensor.view(1, FRAME_SIZE, FRAME_SIZE)


class KeyVocabulary:
    """Maps the free-form OS key names `record.py`'s `pynput` listener
    logs (e.g. `"'a'"`, `"Key.space"`) to stable small integer indices
    for the key-prediction head, built from whatever keys actually
    appear in one session's labels — there's no fixed universal key
    enum to draw from. A key seen at inference time that wasn't in the
    training vocabulary maps to a reserved "unknown" slot rather than
    raising, since a policy trained on one session may see a session
    recorded with slightly different key usage later.
    """

    def __init__(self, keys):
        self.keys = sorted(set(keys))
        self._index = {k: i for i, k in enumerate(self.keys)}

    def index_of(self, key: str) -> int:
        return self._index.get(key, len(self.keys))

    def size(self) -> int:
        return len(self.keys) + 1  # + the unknown slot

    def to_dict(self) -> dict:
        return {"keys": self.keys}

    @classmethod
    def from_dict(cls, data: dict) -> "KeyVocabulary":
        vocab = cls([])
        vocab.keys = list(data["keys"])
        vocab._index = {k: i for i, k in enumerate(vocab.keys)}
        return vocab


def build_key_vocabulary(labels: list) -> KeyVocabulary:
    keys = [label["action"]["key"] for label in labels if label["action"]["type"] == "key"]
    return KeyVocabulary(keys)


def read_labels(session_dir) -> list:
    labels_path = Path(session_dir) / "labels.jsonl"
    return [json.loads(line) for line in labels_path.read_text(encoding="utf-8").splitlines() if line.strip()]


class DemonstrationDataset(Dataset):
    """One `(frame, action_type, click_xy, key_index)` example per
    labeled frame in a session. `click_xy` is `(0, 0)` and `key_index`
    is `-1` (the loss functions' "ignore this" sentinel) whenever
    that head isn't the relevant one for a given example's action —
    see `train`'s masked loss computation."""

    def __init__(self, session_dir, key_vocab: KeyVocabulary):
        self.session_dir = Path(session_dir)
        self.labels = read_labels(session_dir)
        self.key_vocab = key_vocab

    def __len__(self) -> int:
        return len(self.labels)

    def __getitem__(self, idx: int):
        label = self.labels[idx]
        frame = load_frame_tensor(self.session_dir / label["frame"])
        action = label["action"]
        action_type = action_type_index(action)

        click_xy = torch.zeros(2)
        key_index = -1
        if action["type"] == "click":
            click_xy = torch.tensor([float(action["x"]), float(action["y"])])
        elif action["type"] == "key":
            key_index = self.key_vocab.index_of(action["key"])

        return frame, action_type, click_xy, key_index


class BehaviorCloningPolicy(nn.Module):
    """Tiny CNN over one grayscale frame, with three output heads
    matching `label.py`'s action shape: which action type, where to
    click (regression, only meaningful for click actions), and which
    key (classification, only meaningful for key actions). See the
    module docstring for why this architecture is deliberately small
    and unturned."""

    def __init__(self, num_keys: int):
        super().__init__()
        self.backbone = nn.Sequential(
            nn.Conv2d(1, 16, kernel_size=5, stride=2),
            nn.ReLU(),
            nn.Conv2d(16, 32, kernel_size=5, stride=2),
            nn.ReLU(),
            nn.Flatten(),
        )
        with torch.no_grad():
            feature_dim = self.backbone(torch.zeros(1, 1, FRAME_SIZE, FRAME_SIZE)).shape[1]
        self.action_type_head = nn.Linear(feature_dim, len(ACTION_TYPES))
        self.click_head = nn.Linear(feature_dim, 2)
        self.key_head = nn.Linear(feature_dim, num_keys)

    def forward(self, frame: torch.Tensor):
        features = self.backbone(frame)
        return self.action_type_head(features), self.click_head(features), self.key_head(features)


def train(session_dir, epochs: int = 1, batch_size: int = 8, lr: float = 1e-3) -> tuple:
    """Trains a fresh `BehaviorCloningPolicy` on one session directory
    for `epochs` passes and returns `(model, key_vocab, loss_history)` —
    `loss_history` (one average-loss float per epoch) is real training
    output a caller/test can assert is finite and produced, not a
    claim about how *good* the resulting policy is."""
    labels = read_labels(session_dir)
    key_vocab = build_key_vocabulary(labels)
    dataset = DemonstrationDataset(session_dir, key_vocab)
    loader = DataLoader(dataset, batch_size=min(batch_size, len(dataset)), shuffle=True)

    model = BehaviorCloningPolicy(key_vocab.size())
    optimizer = torch.optim.Adam(model.parameters(), lr=lr)
    action_type_loss_fn = nn.CrossEntropyLoss()
    click_loss_fn = nn.MSELoss()
    key_loss_fn = nn.CrossEntropyLoss(ignore_index=-1)

    history = []
    for _ in range(epochs):
        total_loss = 0.0
        batches = 0
        for frame, action_type, click_xy, key_index in loader:
            optimizer.zero_grad()
            type_logits, click_pred, key_logits = model(frame)

            loss = action_type_loss_fn(type_logits, action_type)

            click_mask = action_type == ACTION_TYPES.index("click")
            if click_mask.any():
                loss = loss + click_loss_fn(click_pred[click_mask], click_xy[click_mask])

            # CrossEntropyLoss with ignore_index=-1 skips ignored
            # examples when averaging — but averages over zero examples
            # (every example in this batch not a "key" action) is 0/0,
            # which is NaN, not 0. Guarding with `.any()` here mirrors
            # the click head's guard above rather than trusting
            # ignore_index alone to handle the all-ignored case.
            key_mask = key_index != -1
            if key_mask.any():
                loss = loss + key_loss_fn(key_logits, key_index)

            loss.backward()
            optimizer.step()
            total_loss += loss.item()
            batches += 1
        history.append(total_loss / max(batches, 1))

    return model, key_vocab, history


def save_checkpoint(model: BehaviorCloningPolicy, key_vocab: KeyVocabulary, path) -> None:
    torch.save({"model_state": model.state_dict(), "key_vocab": key_vocab.to_dict()}, path)


def load_checkpoint(path) -> tuple:
    """Returns `(model, key_vocab)` reconstructed from a checkpoint
    written by `save_checkpoint` — used by the future `play` stage, and
    by this module's own round-trip test."""
    checkpoint = torch.load(path, weights_only=False)
    key_vocab = KeyVocabulary.from_dict(checkpoint["key_vocab"])
    model = BehaviorCloningPolicy(key_vocab.size())
    model.load_state_dict(checkpoint["model_state"])
    return model, key_vocab
