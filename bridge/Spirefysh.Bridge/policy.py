#!/usr/bin/env python3
import json
import os
import sys

import torch

import sts2_sim
from train import load, predict, tensors


model, _ = load(sys.argv[1], torch.device("cpu"))
model.eval()
env = sts2_sim.Batch(1)
generator = torch.Generator().manual_seed(int(os.environ.get("SPIREFYSH_POLICY_SEED", "0")))
for line in sys.stdin:
    try:
        request = json.loads(line)
        env.load_snapshot(line)
        descriptors = env.action_descriptors()
        if not descriptors:
            raise ValueError("state has no actions")
        inputs = tensors(env.observe_tokens(), torch.device("cpu"), model)
        with torch.inference_mode():
            logits = predict(model, inputs, "fp32",
                             float(os.environ.get("SPIREFYSH_POLICY_TEMPERATURE", ".8")),
                             flat_policy=True)[0]
        action_flat, legal, width = inputs[5][1][:len(logits)], inputs[5][2][:len(logits)], inputs[5][4]
        actions = action_flat % width
        allowed_kinds = set(request["allowed_kinds"])
        allowed_models = set(request["allowed_models"])
        allowed_targets = set(request["allowed_targets"])
        kinds = [descriptors[int(index)][0] for index in actions]
        kinds = ["take_reward" if kind.startswith("reward_") else
                 "skip_rewards" if kind == "Leave" and "skip_rewards" in allowed_kinds else kind
                 for kind in kinds]
        allowed = torch.tensor([
            (kind in allowed_kinds or "choose_cards" in allowed_kinds) and
            (kind != "map_node" or descriptors[int(index)][2] in allowed_targets) and
            (descriptors[int(index)][0] not in ("play_card", "use_potion") or
             descriptors[int(index)][3] in allowed_models)
            for index, kind in zip(actions, kinds)
        ])
        masked = logits.masked_fill(~legal | ~allowed, -torch.inf)
        if not torch.isfinite(masked).any():
            raise ValueError("policy has no action matching the live legal set")
        selected = torch.multinomial(masked.softmax(0), 1, generator=generator)
        choice = int(actions[selected].item())
        _, source, target, model_id = descriptors[choice]
        kind = kinds[int(selected)]
        print(json.dumps({"index": choice, "kind": kind, "source_id": source,
                          "target_id": target, "model_id": model_id}), flush=True)
    except Exception as error:
        print(json.dumps({"error": f"{type(error).__name__}: {error}"}), flush=True)
