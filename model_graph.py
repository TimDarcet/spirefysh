import base64
import hashlib
import inspect
import json
from collections import OrderedDict, defaultdict
from pathlib import Path

import torch
from torch.utils._python_dispatch import TorchDispatchMode


TRACE_SCHEMA = 1


def feature_schema(model):
    import sts2_sim
    from model import ACTION_FIELDS, SEMANTIC_NAMES, TOKEN_SPECS
    labels = {
        "run": ("ascension", "act", "floor", "bosses visited", "weak encounters",
                "regular encounters", "elite encounters", "HP", "maximum HP", "HP fraction",
                "missing HP fraction", "gold", "energy", "draw", "orb slots", "shop removals",
                "card rarity offset", "potion odds", "unknown combat weight",
                "unknown elite weight", "unknown treasure weight", "unknown shop weight"),
        "card": ("cost delta", "card value", "cost override", "enchantment amount",
                 "enchantment value", "replays"),
        "action": ("expected block", "expected target damage", "expected total damage",
                   "player HP delta", "expected draw", "expected discard", "expected exhaust",
                   "energy cost", "star cost", "action-specific amount"),
    }
    result = {
        "schema": 1,
        "feature_version": model.layout["version"],
        "domains": [
            {"name": name, "categorical_slots": categorical, "numerical_channels": [
                {"index": index, "name": labels.get(name, ())[index]
                 if index < len(labels.get(name, ())) else f"{name} numerical channel {index}"}
                for index in range(numeric)
            ]}
            for name, _unsigned, _signed, categorical, numeric in TOKEN_SPECS
        ],
        "action": {
            "categorical_slots": ACTION_FIELDS[2],
            "numerical_channels": [
                {"index": index, "name": labels["action"][index]}
                for index in range(ACTION_FIELDS[3])
            ],
        },
        "semantic_namespaces": [
            {"name": name, "size": model.layout[name + "_semantic_count"]}
            for name in SEMANTIC_NAMES
        ],
    }
    if hasattr(sts2_sim, "feature_schema"):
        result["native"] = sts2_sim.feature_schema()
    return result


def _tensors(value):
    if isinstance(value, torch.Tensor):
        yield value
    elif isinstance(value, (tuple, list)):
        for item in value:
            yield from _tensors(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from _tensors(item)


def _hash_tensor(value):
    value = value.detach().cpu().contiguous()
    return hashlib.sha256(value.numpy().tobytes()).hexdigest()


def _source_hash(path):
    try:
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    except OSError:
        return None


def _short_op(name):
    return name.removeprefix("aten.").split(".", 1)[0].replace("_", " ")


class OperationRecorder(TorchDispatchMode):
    def __init__(self, model):
        super().__init__()
        from model import _GRAPH_RECORDER, _GRAPH_SCOPES
        self.model = model
        self.graph_recorder = _GRAPH_RECORDER
        self.graph_scopes = _GRAPH_SCOPES
        self.graph_token = None
        self.ops = []
        self.tensors = {}
        self.values = {}
        self.tensor_ids = {}
        self.producers = {}
        self.stack = []
        self.calls = defaultdict(int)
        self.region_calls = defaultdict(int)
        self.module_types = {name: type(module).__name__ for name, module in model.named_modules()}
        self.module_calls = {}
        self.handles = []

    def region(self, name):
        call = self.region_calls[name]
        self.region_calls[name] += 1
        return f"{name}#{call}"

    def _tensor(self, value, leaf=True):
        key = id(value)
        if key in self.tensor_ids:
            return self.tensor_ids[key]
        tensor_id = f"t{len(self.tensors)}"
        self.tensor_ids[key] = tensor_id
        self.tensors[tensor_id] = {
            "id": tensor_id, "shape": list(value.shape), "dtype": str(value.dtype),
            "device": str(value.device), "leaf": leaf,
        }
        self.values[tensor_id] = value
        return tensor_id

    def _source(self):
        for frame in reversed(inspect.stack(context=0)):
            if Path(frame.filename).name == "model.py":
                return {"file": frame.filename, "line": frame.lineno, "function": frame.function}
        return None

    def __torch_dispatch__(self, func, _types, args=(), kwargs=None):
        inputs = [self._tensor(value) for value in _tensors((args, kwargs or {}))]
        output = func(*args, **(kwargs or {}))
        outputs = [self._tensor(value, False) for value in _tensors(output)]
        op_id = f"o{len(self.ops)}"
        source = self._source()
        module = self.stack[-1] if self.stack else None
        self.ops.append({
            "id": op_id, "op": str(func), "inputs": inputs, "outputs": outputs,
            "module": module[0] if module else None, "call": module[1] if module else None,
            "region": self.graph_scopes.get(),
            "source": source,
        })
        for tensor_id in outputs:
            self.producers[tensor_id] = op_id
        return output

    def __enter__(self):
        self.graph_token = self.graph_recorder.set(self)
        for name, module in self.model.named_modules():
            def before(_module, _args, name=name):
                call = self.calls[name]; self.calls[name] += 1
                self.stack.append((name, call))
                self.module_calls[name, call] = {"args": _args}

            def after(_module, _args, output, name=name):
                if not self.stack or self.stack[-1][0] != name:
                    raise RuntimeError("model graph module stack is inconsistent")
                _name, call = self.stack.pop()
                self.module_calls[_name, call]["output"] = output
                return output

            self.handles.append(module.register_forward_pre_hook(before))
            self.handles.append(module.register_forward_hook(after))
        return super().__enter__()

    def __exit__(self, *args):
        try:
            return super().__exit__(*args)
        finally:
            for handle in self.handles:
                handle.remove()
            self.handles.clear()
            self.graph_recorder.reset(self.graph_token)
            self.graph_token = None

    def semantic_graph(self, output_tensors):
        groups, current, counters = [], None, defaultdict(int)
        for op in self.ops:
            source = op["source"] or {}
            region = op["region"][-1] if op["region"] else None
            if op["module"]:
                base = ("module", op["module"], op["call"])
            else:
                base = ("source", region or source.get("function", "runtime"),
                        source.get("line", 0))
            if current is None or current["base"] != base:
                ordinal = counters[base]; counters[base] += 1
                node_id = f"n{len(groups)}"
                module = op["module"]
                kind = self.module_types.get(module, _short_op(op["op"])) if module else _short_op(op["op"])
                label = f"{module or source.get('function', 'runtime')} · {kind}"
                if region:
                    scope = region
                elif module and module.startswith("encoders."):
                    scope = ".".join(module.split(".")[:2])
                elif module and module.startswith("global_transformer.layers."):
                    scope = ".".join(module.split(".")[:3])
                elif module:
                    scope = module.split(".", 1)[0] or "model"
                else:
                    scope = region or source.get("function", "runtime")
                current = {
                    "id": node_id, "base": base, "label": label, "kind": kind,
                    "module": module, "call": op["call"], "ordinal": ordinal,
                    "scope": scope, "source": op["source"], "op_ids": [],
                    "inputs": [], "outputs": [],
                }
                groups.append(current)
            current["op_ids"].append(op["id"])

        counts, previous, segment = defaultdict(int), None, None
        for node in groups:
            base = node["scope"]
            if base != previous:
                segment = counts[base]; counts[base] += 1
            node["scope_name"] = base.rsplit("#", 1)[0]
            node["scope"] = f"{base}@{segment}"
            previous = base
        owner = {op_id: node["id"] for node in groups for op_id in node["op_ids"]}
        edges = {}
        output_set = set(output_tensors)
        for node in groups:
            op_ids = set(node["op_ids"])
            inputs, outputs = set(), set()
            for op_id in node["op_ids"]:
                op = self.ops[int(op_id[1:])]
                inputs.update(tensor for tensor in op["inputs"]
                              if self.producers.get(tensor) not in op_ids)
                for tensor in op["outputs"]:
                    consumers = [candidate["id"] for candidate in self.ops
                                 if tensor in candidate["inputs"]]
                    if tensor in output_set or any(consumer not in op_ids for consumer in consumers):
                        outputs.add(tensor)
            node["inputs"], node["outputs"] = sorted(inputs), sorted(outputs)
            node.pop("base")
            for tensor in inputs:
                producer = self.producers.get(tensor)
                if producer and owner[producer] != node["id"]:
                    key = owner[producer], node["id"]
                    edges.setdefault(key, []).append(tensor)
        return groups, [
            {"from": left, "to": right, "tensors": tensors}
            for (left, right), tensors in edges.items()
        ]


def trace_model(model, inputs, checkpoint_sha256, observation_digest, simulator_path,
                temperature=1):
    def forward(*args):
        return model(*args, flat_policy=True, temperature=temperature)

    model.eval(); model._graph_cache.clear(); model._trace_graph = False
    with torch.no_grad():
        normal = forward(*inputs[:6])
    model._graph_cache.clear(); model._trace_graph = True
    try:
        with torch.no_grad():
            uncached = forward(*inputs[:6])
        recorder = OperationRecorder(model)
        with torch.no_grad(), recorder:
            traced = torch.func.functionalize(forward)(*inputs[:6])
    finally:
        model._trace_graph = False
        model._graph_cache.clear()

    trace_error = max(float((left - right).abs().max())
                      for left, right in zip(normal, traced))
    if any(not torch.equal(left, right) for left, right in zip(normal, uncached)) \
            or trace_error > 1e-6:
        raise RuntimeError("model graph trace does not reproduce ordinary inference")
    output_ids = [recorder._tensor(value, False) for value in _tensors(traced)]
    nodes, edges = recorder.semantic_graph(output_ids)
    if len({op for node in nodes for op in node["op_ids"]}) != len(recorder.ops):
        raise RuntimeError("model graph trace has incomplete operation coverage")
    rewrites, rewrite_error = [], 0.0
    modules = dict(model.named_modules())
    with torch.no_grad():
        for (name, call), record in recorder.module_calls.items():
            module = modules[name]
            if type(module).__name__ != "TokenEncoder":
                continue
            categorical, numeric, embeddings = record["args"]
            projected = module.numeric(numeric) if module.numeric is not None else \
                embeddings.new_zeros((len(categorical), len(module.norm.weight)))
            expected = module.norm(torch.nn.functional.embedding(
                categorical.long(), embeddings,
            ).sum(1) + projected)
            actual = record["output"]
            error = float((expected - actual).abs().max()) if actual.numel() else 0.0
            rewrite_error = max(rewrite_error, error)
            rewrites.append({
                "scope": name, "call": call,
                "label": f"{name} · categorical counts + numerical channels → Linear → LayerNorm",
                "kind": "fused_token_encoder", "maximum_error": error,
            })
    if rewrite_error > 1e-6:
        raise RuntimeError("model graph semantic rewrite verification failed")
    for rewrite in rewrites:
        for node in nodes:
            if node["module"] == rewrite["scope"]:
                node["label"] = rewrite["label"]
                node["kind"] = rewrite["kind"]
    graph = {
        "schema": TRACE_SCHEMA,
        "nodes": nodes,
        "edges": edges,
        "execution": [node["id"] for node in nodes],
        "raw_ops": recorder.ops,
        "tensors": recorder.tensors,
        "outputs": output_ids,
        "rewrites": rewrites,
    }
    proof = {
        "verified": True,
        "checkpoint_sha256": checkpoint_sha256,
        "observation_digest": int(observation_digest),
        "model_version": model.model_version,
        "feature_version": model.layout["version"],
        "model_source_sha256": _source_hash(inspect.getsourcefile(type(model))),
        "simulator_sha256": _source_hash(simulator_path),
        "torch_version": torch.__version__,
        "trace_schema": TRACE_SCHEMA,
        "raw_operations": len(recorder.ops),
        "semantic_nodes": len(nodes),
        "operation_coverage": 1.0,
        "semantic_partition_coverage": 1.0,
        "verified_rewrites": len(rewrites),
        "maximum_rewrite_error": rewrite_error,
        "maximum_output_error": trace_error,
        "output_sha256": [_hash_tensor(value) for value in normal],
        "traced_output_sha256": [_hash_tensor(value) for value in traced],
        "proof_target": "cpu/fp32",
    }
    proof["attestation_sha256"] = hashlib.sha256(json.dumps(
        proof, sort_keys=True, separators=(",", ":"),
    ).encode()).hexdigest()
    graph_id = hashlib.sha256(json.dumps({
        "proof": proof, "nodes": nodes, "edges": edges,
    }, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"graph_id": graph_id, "graph": graph, "proof": proof}, recorder


def tensor_detail(value):
    value = value.detach().cpu().float().contiguous()
    finite = torch.isfinite(value)
    selected = value[finite]
    summary = {"shape": list(value.shape), "dtype": str(value.dtype)}
    if len(selected):
        summary |= {
            "mean": float(selected.mean()), "stddev": float(selected.std(unbiased=False)),
            "rms": float(selected.square().mean().sqrt()), "minimum": float(selected.min()),
            "maximum": float(selected.max()),
        }
    summary["data"] = base64.b64encode(value.numpy().tobytes()).decode()
    return summary


class GraphCache:
    def __init__(self, capacity=8):
        self.capacity = capacity
        self.values = OrderedDict()

    def put(self, graph_id, value):
        self.values[graph_id] = value
        self.values.move_to_end(graph_id)
        removed = []
        while len(self.values) > self.capacity:
            removed.append(self.values.popitem(last=False)[0])
        return removed

    def get(self, graph_id):
        value = self.values[graph_id]
        self.values.move_to_end(graph_id)
        return value
