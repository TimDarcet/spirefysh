#!/usr/bin/env python3
import json
import shutil
import sys

import torch

import sts2_sim


def serve(path):
    from model import load, predict, tensors

    torch.set_num_threads(1)
    target = torch.device("cpu")
    model, _ = load(path, target)
    model.eval()
    env = sts2_sim.Batch(1)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            env.load_snapshot(line)
            descriptors = env.action_descriptors()
            with torch.inference_mode():
                current = predict(model, tensors(env.observe_tokens(), target, model), "fp32")[1].sigmoid().item()
            phase = env.stats()[0][4]
            current = 1.0 if phase == 12 else 0.0 if phase == 13 else current
            selector = request.get("selector", {})
            if not selector:
                print(json.dumps({"win_probability": current, "delta": 0}), flush=True)
                continue
            env.load_snapshot_actions(line)
            with torch.inference_mode():
                values = predict(model, tensors(env.observe_tokens(), target, model), "fp32")[1].sigmoid().tolist()
            values = [1.0 if row[4] == 12 else 0.0 if row[4] == 13 else value
                      for row, value in zip(env.stats(), values)]
            matches = [i for i, (kind, source, target_id, model_id) in enumerate(descriptors) if all(
                selector.get(key) is None or selector[key] == value
                for key, value in (("kind", kind), ("source_id", source),
                                   ("target_id", target_id), ("model_id", model_id))
            )]
            if selector and not matches:
                raise ValueError("action is not currently available")
            best = max(values, default=current)
            value = max(values[i] for i in matches)
            print(json.dumps({"win_probability": value, "delta": value - best}), flush=True)
        except Exception as error:
            print(json.dumps({"error": str(error)}), flush=True)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "export":
        shutil.copyfile(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 3 and sys.argv[1] == "serve":
        serve(sys.argv[2])
    else:
        raise SystemExit(f"usage: {sys.argv[0]} export SOURCE DESTINATION | serve MODEL")
