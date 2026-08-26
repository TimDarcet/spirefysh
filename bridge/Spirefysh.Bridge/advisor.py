#!/usr/bin/env python3
import json
import sys
from pathlib import Path

import torch
from torch import nn

import sts2_sim


ATOMS = 51


class Agent(nn.Module):
    def __init__(self, state_size, action_size, layout):
        super().__init__()
        card, zones, slots, card_values, draw, sides, draw_slots, draw_values = layout[:8]
        cards = layout[8]
        self.card_slice = slice(card, card + zones * slots * card_values)
        self.draw_slice = slice(draw, draw + sides * draw_slots * draw_values)
        self.card_shape = zones, slots, card_values
        self.draw_shape = sides, draw_slots, draw_values
        dense_size = state_size - zones * slots * card_values - sides * draw_slots * draw_values
        self.card_id = nn.Embedding(cards + 1, 32, padding_idx=0)
        self.card = nn.Linear(card_values - 3, 32)
        self.position = nn.Embedding(draw_slots, 8)
        self.draw = nn.Sequential(nn.Linear(40, 32), nn.ReLU())
        self.register_buffer("draw_position", torch.arange(draw_slots), persistent=False)
        self.state = nn.Sequential(nn.Linear(dense_size + zones * 33 + sides * 33, 256), nn.ReLU(), nn.Linear(256, 128), nn.ReLU())
        self.action = nn.Sequential(nn.Linear(action_size, 64), nn.ReLU())
        self.policy = nn.Sequential(nn.Linear(192, 128), nn.ReLU(), nn.Linear(128, ATOMS))
        self.progress = nn.Sequential(nn.Linear(192, 128), nn.ReLU(), nn.Linear(128, 1))
        self.value = nn.Linear(128, 1)

    @staticmethod
    def pool(encoded, mask, weights=None):
        weights = mask if weights is None else weights * mask
        count = weights.sum(-1, keepdim=True)
        return torch.cat(((encoded * weights[..., None]).sum(-2) / count.clamp_min(1).sqrt(), count / mask.shape[-1]), -1)

    def encode_cards(self, cards):
        ids = (cards[..., 0] * 65536).round().long().clamp(0, self.card_id.num_embeddings - 1)
        return torch.relu(self.card(cards[..., 1:-2]) + self.card_id(ids))

    def q(self, states, actions):
        cards = states[:, self.card_slice].reshape(states.shape[0], *self.card_shape)
        draws = states[:, self.draw_slice].reshape(states.shape[0], *self.draw_shape)
        card_mask, draw_mask = cards[..., 0] != 0, draws[..., 0] != 0
        counts = cards[..., -2] * 65535 + cards[..., -1] * 4294901760
        cards = self.pool(self.encode_cards(cards), card_mask, counts).flatten(1)
        positions = self.position(self.draw_position)[None, None].expand(*draws.shape[:-1], 8)
        draws = self.pool(self.draw(torch.cat((self.encode_cards(draws[..., 1:]), positions), -1)), draw_mask).flatten(1)
        dense = torch.cat((states[:, :self.card_slice.start], states[:, self.card_slice.stop:self.draw_slice.start], states[:, self.draw_slice.stop:]), -1)
        state = self.state(torch.cat((dense, cards, draws), -1))[:, None].expand(-1, actions.shape[1], -1)
        logits = self.policy(torch.cat((state, self.action(actions)), -1))
        return (logits.softmax(-1) * torch.linspace(0, 1, ATOMS)).sum(-1)


def load(path):
    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    if checkpoint.get("architecture") != 3 or checkpoint.get("feature_version") != 16:
        raise ValueError("unsupported model")
    model = Agent(checkpoint["state_size"], checkpoint["action_size"], checkpoint["layout"])
    model.load_state_dict(checkpoint["model"])
    model.eval()
    env = sts2_sim.Batch(1)
    if env.sizes() != (checkpoint["state_size"], checkpoint["action_size"]) or tuple(env.feature_layout()) != tuple(checkpoint["layout"]) or env.fingerprint() != checkpoint["fingerprint"]:
        raise ValueError("model and simulator do not match")
    return model, env


def serve(path):
    torch.set_num_threads(1)
    model, env = load(path)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            env.load_snapshot(line)
            states, actions, mask, _ = env.observe()
            with torch.inference_mode():
                values = model.q(torch.from_numpy(states), torch.from_numpy(actions))[0]
            descriptors = env.action_descriptors()
            selector = request.get("selector", {})
            matches = [i for i, (kind, source, target, model_id) in enumerate(descriptors) if all(
                selector.get(key) is None or selector[key] == value
                for key, value in (("kind", kind), ("source_id", source), ("target_id", target), ("model_id", model_id))
            ) and mask[0][i]]
            if not matches:
                raise ValueError("action is not currently available")
            value = max(float(values[i]) for i in matches)
            best = max(float(values[i]) for i, allowed in enumerate(mask[0]) if allowed)
            print(json.dumps({"win_probability": value, "delta": value - best}), flush=True)
        except Exception as error:
            print(json.dumps({"error": str(error)}), flush=True)


def export(source, destination):
    checkpoint = torch.load(source, map_location="cpu", weights_only=False)
    Path(destination).parent.mkdir(parents=True, exist_ok=True)
    torch.save({key: checkpoint[key] for key in ("model", "state_size", "action_size", "layout", "architecture", "feature_version", "fingerprint")}, destination)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "export":
        export(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 3 and sys.argv[1] == "serve":
        serve(sys.argv[2])
    else:
        raise SystemExit(f"usage: {sys.argv[0]} export SOURCE DESTINATION | serve MODEL")
