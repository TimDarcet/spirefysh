import math

import torch
from torch.nn import functional as F

from model import _norm_head, predict, tensors


def _attention(layer, value, key_padding=None, mask=None):
    value = layer.norm1(value) if layer.norm_first else value
    batch, length, width = value.shape
    heads = layer.self_attn.num_heads
    query, key, _ = F.linear(
        value, layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias,
    ).reshape(batch, length, 3, heads, width // heads).permute(2, 0, 3, 1, 4)
    score = query @ key.transpose(-1, -2) / math.sqrt(width // heads)
    if key_padding is not None:
        score = score.masked_fill(key_padding[:, None, None, :], -torch.inf) \
            if key_padding.dtype == torch.bool else score + key_padding[:, None, None, :]
    if mask is not None:
        score = score + mask if mask.dtype != torch.bool else score.masked_fill(mask, -torch.inf)
    return score.softmax(-1)[0].float().detach().cpu()


def _map_attention(query, key_value, sequence, heads):
    offsets, rows, position, maximum = sequence
    lengths = offsets[1:] - offsets[:-1]
    width = query.shape[1] // heads
    query = query.reshape(len(query), heads, width)
    key = key_value.reshape(len(key_value), 2, heads, width)[:, 0]
    padded = key.new_zeros((len(query), maximum, heads, width))
    padded[rows, position] = key
    score = torch.einsum("qhd,qlhd->hql", query, padded) / math.sqrt(width)
    valid = torch.arange(maximum, device=query.device)[None] < lengths[:, None]
    return score.masked_fill(~valid[None], -torch.inf).softmax(-1).float().detach().cpu()


def _items(state, group, count):
    base = state.get("common", state)
    rows = {
        "deck": base.get("deck", []),
        "hand": state.get("hand", []),
        "draw": state.get("draw", []),
        "discard": state.get("discard", []),
        "exhaust": state.get("exhaust", []),
        "relic": base.get("relics", []),
        "potion": base.get("potions", []),
        "orb": state.get("orbs", []),
    }.get(group, [])
    labels = []
    for index, row in enumerate(rows[:count]):
        value = row if isinstance(row, str) or row is None else row.get("id")
        label = "empty" if value is None else value
        labels.append({"label": label, "group": group, "index": index})
        if isinstance(row, dict) and row.get("instance") is not None:
            labels[-1]["key"] = f"combat-card:{max(0, row['instance'] - 1)}"
        elif group == "potion":
            labels[-1]["key"] = f"potion-slot:{index}"
    return labels


def _labels(segments, state, descriptors):
    labels = [{"label": "critic state", "group": "state", "kind": "critic"}]
    for group, count in segments:
        if group == "action":
            labels.extend({
                "label": action[3] or action[0], "group": "action", "kind": "action",
                "action": index, "source": action[1], "target": action[2],
            } for index, action in enumerate(descriptors[:count]))
            continue
        items = _items(state, group, count)
        if group == "actor":
            actors = [("player", None)]
            if state.get("osty", {}).get("max_hp", 0) > 0:
                actors.append(("osty", None))
            actors += [(enemy.get("id", "enemy"), f"creature:{max(0, enemy['instance'] - 1)}")
                       for enemy in state.get("enemies", [])]
            items = [{"label": label, "group": group, "index": index, "key": key}
                     for index, (label, key) in enumerate(actors[:count])]
        labels.extend(items)
        labels.extend({"label": f"{group} {index + 1}", "group": group, "index": index}
                      for index in range(len(items), count))
    return labels


def _capture(model, inputs, temperature, state, descriptors):
    attention, handles, global_input = [], [], {}
    calls = {}

    def layer_hook(name):
        def capture(layer, args, kwargs):
            key = name.rsplit(".layers.", 1)[0]
            call = calls.get(name, 0); calls[name] = call + 1
            weights = _attention(
                layer, args[0], kwargs.get("src_key_padding_mask"), kwargs.get("src_mask"),
            )
            attention.append({
                "hierarchy": key, "layer": int(name.rsplit(".", 1)[1]), "call": call,
                "weights": weights.tolist(),
            })
        return capture

    for name, module in model.named_modules():
        if isinstance(module, torch.nn.TransformerEncoderLayer):
            handles.append(module.register_forward_pre_hook(layer_hook(name), with_kwargs=True))

    def global_hook(_module, args, kwargs):
        global_input["value"] = args[0].detach()
        global_input["mask"] = kwargs.get("src_key_padding_mask")

    handles.append(model.global_transformer.register_forward_pre_hook(global_hook, with_kwargs=True))
    original_attend = model.attend
    had_attend = "attend" in model.__dict__
    map_call = 0
    def attend(query, key_value, sequence):
        nonlocal map_call
        weights = _map_attention(query, key_value, sequence, model.heads)
        attention.append({
            "hierarchy": "map", "layer": map_call, "call": 0,
            "weights": weights.tolist(),
        })
        map_call += 1
        return original_attend(query, key_value, sequence)

    model.attend = attend
    model._graph_cache.clear()
    model._explanation_segments = []
    try:
        with torch.no_grad():
            policy, critic = predict(model, inputs, "fp32", temperature, flat_policy=True)
        segments = model._explanation_segments
    finally:
        del model._explanation_segments
        if had_attend:
            model.attend = original_attend
        else:
            del model.attend
        for handle in handles:
            handle.remove()
    labels = _labels(segments, state, descriptors)
    if len(labels) != global_input["value"].shape[1]:
        raise ValueError("activation labels do not align with the global sequence")
    for record in attention:
        length = len(record["weights"][0]) if record["weights"] else 0
        record["labels"] = labels if record["hierarchy"] == "global_transformer" \
            and length == len(labels) else [
                {"label": f"token {index}", "group": record["hierarchy"]}
                for index in range(length)
            ]
    return policy, critic.squeeze(), global_input["value"], labels, attention


def _evaluate(model, sequence, labels, temperature, cases):
    results = []
    for start in range(0, len(cases), 32):
        removed = cases[start:start + 32]
        kept = [[index for index in range(len(labels)) if index not in drop] for drop in removed]
        length = max(map(len, kept))
        batch = sequence.new_zeros((len(kept), length, sequence.shape[-1]))
        padding = torch.ones((len(kept), length), dtype=torch.bool, device=sequence.device)
        for row, indices in enumerate(kept):
            batch[row, :len(indices)] = sequence[0, indices]
            padding[row, :len(indices)] = False
        transformed = model.global_transformer(batch, src_key_padding_mask=padding)
        for row, indices in enumerate(kept):
            kept_labels = [labels[index] for index in indices]
            positions = [index for index, token in enumerate(kept_labels)
                         if token.get("kind") == "action"]
            ids = [kept_labels[index]["action"] for index in positions]
            logits = _norm_head(
                transformed[row, positions], model.global_norm, model.policy,
            ) / temperature
            results.append((
                ids, logits, logits.log_softmax(0),
                _norm_head(transformed[row, :1], model.global_norm, model.critic)[0],
            ))
    return results


def _case_attention(model, sequence, labels, removed):
    kept = [index for index in range(len(labels)) if index not in removed]
    records, handles = [], []
    for layer_index, layer in enumerate(model.global_transformer.layers):
        def capture(module, args, kwargs, index=layer_index):
            records.append({
                "hierarchy": "global_transformer", "layer": index, "call": 0,
                "weights": _attention(
                    module, args[0], kwargs.get("src_key_padding_mask"), kwargs.get("src_mask"),
                ).tolist(),
                "labels": [labels[position] for position in kept],
            })
        handles.append(layer.register_forward_pre_hook(capture, with_kwargs=True))
    try:
        with torch.no_grad():
            model.global_transformer(sequence[:, kept])
    finally:
        for handle in handles:
            handle.remove()
    return records


def explain(model, observation, state, descriptors, trace, step, case_id=None):
    model.train(); model.eval()
    inputs = tensors(observation, torch.device("cpu"), model)
    temperature = float(trace["temperature"])
    policy, critic, sequence, labels, attention = _capture(
        model, inputs, temperature, state, descriptors,
    )
    expected_policy = torch.tensor(trace["log_policies"][step])
    expected_critic = float(trace["critic_values"][step])
    if len(policy) != len(expected_policy) or (policy - expected_policy).abs().max().item() > 1e-4 \
            or abs(float(critic) - expected_critic) > 1e-4:
        raise ValueError("checkpoint no longer reproduces this decision")
    chosen = int(trace["choices"][step])
    if len(policy) < 2:
        raise ValueError("forced decisions do not have an alternative action to explain")
    alternative = max((index for index in range(len(policy)) if index != chosen),
                      key=lambda index: float(policy[index]), default=chosen)
    baseline_margin = float(policy[chosen] - policy[alternative])
    baseline_preferred = int(policy.argmax())
    cases = []
    groups = {}
    individual = {"deck", "hand", "draw", "discard", "exhaust", "relic", "potion",
                  "orb", "actor", "effect"}
    for position, token in enumerate(labels):
        if token.get("kind") in ("critic", "action"):
            continue
        groups.setdefault(token["group"], []).append(position)
        if token["group"] in individual:
            cases.append((
                f"{token['group']}:{token.get('index', position)}", token["label"], [position],
            ))
    cases += [(f"group:{group}", group, positions) for group, positions in groups.items()]
    action_positions = {token["action"]: position for position, token in enumerate(labels)
                        if token.get("kind") == "action"}
    specs = []
    for case_key, label, positions in cases:
        related = {token["action"] for token in (labels[position] for position in positions)
                   if token.get("action") is not None}
        keys = {labels[position].get("key") for position in positions
                if labels[position].get("key") is not None}
        related |= {token["action"] for token in labels if token.get("kind") == "action"
                    and (token.get("source") in keys or token.get("target") in keys)}
        protected = related & {chosen, alternative}
        removed_actions = sorted(related - protected)
        removed = set(positions) | {action_positions[action] for action in removed_actions}
        specs.append((case_key, label, positions, protected, removed_actions, removed))
    results = []
    with torch.no_grad():
        evaluated = _evaluate(model, sequence, labels, temperature,
                              [spec[-1] for spec in specs])
        for spec, (ids, logits, log_policy, value) in zip(specs, evaluated):
            case_key, label, positions, protected, removed_actions, _removed = spec
            lookup = {action: index for index, action in enumerate(ids)}
            if chosen not in lookup or alternative not in lookup:
                results.append({
                    "id": case_key, "label": label, "status": "unavailable",
                    "reason": "comparison action removed by dependency closure",
                })
                continue
            chosen_index, alternative_index = lookup[chosen], lookup[alternative]
            margin = float(logits[chosen_index] - logits[alternative_index])
            probability = float(log_policy[chosen_index].exp())
            critic_delta = float(value - critic)
            margin_delta = margin - baseline_margin
            changed = bool(removed_actions)
            partial = protected or labels[positions[0]]["group"] in ("phase", "map", "continuation")
            status = "context_only" if partial else "dependency_closure" if changed else "complete"
            flags = []
            if int(ids[int(logits.argmax())]) != baseline_preferred:
                flags.append("preferred_action_changed")
            if changed:
                flags.append("probability_denominator_changed")
            if status == "context_only":
                flags.append("partial_intervention")
                if abs(margin_delta) < .01:
                    flags.append("compared_context_ignored")
            if abs(margin_delta) >= .01 and abs(critic_delta) >= .01 \
                    and margin_delta * critic_delta < 0:
                flags.append("policy_critic_opposition")
            results.append({
                "id": case_key, "label": label, "group": labels[positions[0]]["group"],
                "status": status, "removed_tokens": len(positions),
                "removed_actions": removed_actions, "action_set_changed": changed,
                "margin": margin, "margin_delta": margin_delta,
                "chosen_probability": probability,
                "chosen_probability_delta": probability - float(policy[chosen].exp()),
                "critic": float(value), "critic_delta": critic_delta, "flags": flags,
            })
    results.sort(key=lambda row: abs(row.get("margin_delta", 0)), reverse=True)
    results += [
        {"id": "state:critic", "label": "critic state", "status": "unavailable",
         "reason": "the critic output requires its state token"},
        {"id": f"action:{chosen}", "label": descriptors[chosen][3] or descriptors[chosen][0],
         "status": "unavailable", "reason": "the chosen action defines the comparison"},
        {"id": f"action:{alternative}",
         "label": descriptors[alternative][3] or descriptors[alternative][0],
         "status": "unavailable", "reason": "the alternative action defines the comparison"},
    ]
    flags = ["near_tie"] if abs(float(policy[chosen].exp() - policy[alternative].exp())) < .05 else []
    result = {
        "schema": 1, "step": step, "chosen": chosen, "alternative": alternative,
        "baseline": {
            "margin": baseline_margin, "chosen_probability": float(policy[chosen].exp()),
            "alternative_probability": float(policy[alternative].exp()),
            "critic": float(critic),
        },
        "labels": labels, "influences": results, "attention": attention, "flags": flags,
    }
    if case_id is not None:
        selected = next((spec for spec in specs if spec[0] == case_id), None)
        if selected is None:
            raise ValueError("unknown deletion case")
        result["attention_case"] = case_id
        result["case_attention"] = _case_attention(model, sequence, labels, selected[-1])
    return result
