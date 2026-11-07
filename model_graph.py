import base64
import hashlib
import inspect
import json
import math
import os
import sys
from collections import OrderedDict, defaultdict
from pathlib import Path

import sts2_sim
import torch
from torch.utils._python_dispatch import TorchDispatchMode


TRACE_SCHEMA = 2
NODE_BUDGET = 30
PLUMBING = {
    "_to_copy", "_unsafe_view", "alias", "cat", "clone", "constant_pad_nd", "contiguous", "copy",
    "detach", "expand", "fill", "index", "index_copy", "index_put", "index_select", "lift_fresh",
    "permute", "repeat", "reshape", "select", "select_scatter", "slice", "slice_scatter", "split",
    "split_with_sizes", "squeeze", "stack", "t", "transpose", "unbind", "unsafe_chunk",
    "unsafe_split", "unsafe_split_with_sizes", "unsqueeze", "view", "zero",
}
GATHERS = {"embedding", "index", "index_select"}
AGGREGATES = {"index_add", "index_reduce", "scatter_add", "scatter_reduce"}
KINDS = {
    "addmm": "Linear", "mm": "Linear", "bmm": "MatMul", "native_layer_norm": "LayerNorm",
    "embedding": "Embedding", "index": "Embedding", "index_select": "Embedding", "gelu": "GELU",
    "relu": "ReLU", "add": "Add", "sub": "Subtract", "mul": "Multiply", "div": "Divide",
    "sum": "Sum", "index_add": "Scatter-add", "scatter_reduce": "Scatter-reduce", "gru": "GRU",
}
CHANNELWISE = {"abs", "add", "clamp", "div", "exp", "gelu", "log", "masked_fill", "mul",
               "native_layer_norm", "neg", "relu", "sub", "where"}
PARAMETRIC = {"Attention", "Embedding", "GRU", "LayerNorm", "Linear"}
_CALL_IMPL = torch.nn.Module._call_impl.__code__


NUMERIC_CHANNELS = {name: tuple(fields) for name, _codes, fields in sts2_sim.feature_schema()["domains"]}


def feature_schema(model):
    from model import SEMANTIC_NAMES
    native = sts2_sim.feature_schema()
    return {
        "schema": 2,
        "feature_version": model.layout["version"],
        "domains": [
            {"name": name, "code_slots": codes, "numerical_channels": [
                {"index": index, "name": field} for index, field in enumerate(fields)
            ]}
            for name, codes, fields in native["domains"]
        ],
        "semantic_namespaces": [
            {"name": name, "size": model.layout[name + "_semantic_count"]}
            for name in SEMANTIC_NAMES
        ],
        "native": native,
    }


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


def _base(op):
    return op["op"].split(".")[1]


def _humanize(name):
    return name.replace("_", " ")


class OperationRecorder(TorchDispatchMode):
    def __init__(self, model):
        super().__init__()
        from model import _GRAPH_RECORDER, graph_name
        self.model, self.graph_recorder, self.graph_token = model, _GRAPH_RECORDER, None
        self.hook = graph_name.__code__
        self.ops, self.calls, self.tensors, self.values, self.tensor_ids = [], [], {}, {}, {}
        self.names, self.scopes, self.frames = {}, {}, {}
        self.labels, self.channels, self.vocabularies, self.virtual = {}, {}, {}, {}
        self.parameters = {id(value): name for name, value in model.named_parameters()}
        self.modules = {id(module): (name, module) for name, module in model.named_modules()
                        if module is not model}
        self.source_file = inspect.getsourcefile(type(model))

    def _tensor(self, value):
        if id(value) not in self.tensor_ids:
            tensor_id = self.tensor_ids[id(value)] = f"t{len(self.tensors)}"
            self.tensors[tensor_id] = {"id": tensor_id, "shape": list(value.shape),
                                       "dtype": str(value.dtype)}
            self.values[tensor_id] = value
        return self.tensor_ids[id(value)]

    def name(self, value, name):
        if torch._is_functional_tensor(value):
            torch._sync(value)
            value = torch._from_functional_tensor(value)
        self.names[self._tensor(value)] = name

    def _scope(self, frame, parent):
        if frame not in self.frames:
            code = frame.f_code
            if code is _CALL_IMPL:
                name, module = self.modules[id(frame.f_locals["self"])]
                parts = name.split(".")
                container = isinstance(self.model.get_submodule(".".join(parts[:-1])),
                                       (torch.nn.ModuleList, torch.nn.ModuleDict))
                scope = {"label": _humanize(" ".join(parts[-2:] if container else parts[-1:])),
                         "kind": type(module).__name__, "code": name, "module": name}
            else:
                args = [frame.f_locals.get(name) for name in code.co_varnames[:code.co_argcount]]
                qualifier = next((arg for arg in args if isinstance(arg, str)), None) or next(
                    (self.modules[id(arg)][0] for arg in reversed(args) if id(arg) in self.modules),
                    None)
                scope = {"label": _humanize(code.co_name.strip("_"))
                         + (f" · {_humanize(qualifier)}" if qualifier else ""),
                         "kind": "function", "code": code.co_name, "qualified": bool(qualifier)}
            self.frames[frame] = key = f"s{len(self.scopes)}"
            self.scopes[key] = scope | {"id": key, "parent": parent or "root"}
        return self.frames[frame]

    def _context(self):
        frames, source, frame = [], None, sys._getframe(2)
        while frame is not None:
            code = frame.f_code
            if frame in self.frames or code is _CALL_IMPL \
                    and id(frame.f_locals.get("self")) in self.modules \
                    or code.co_filename == self.source_file and code is not self.hook \
                    and code.co_name != "forward" and not code.co_name.startswith("<"):
                frames.append(frame)
            if source is None and code.co_filename == self.source_file:
                source = {"file": code.co_filename, "line": frame.f_lineno,
                          "function": code.co_name}
            frame = frame.f_back
        scope = "root"
        for frame in reversed(frames):
            scope = self._scope(frame, scope)
        return scope, source

    def __torch_dispatch__(self, func, _types, args=(), kwargs=None):
        kwargs = kwargs or {}
        inputs = [self._tensor(value) for value in _tensors((args, kwargs))]
        output = func(*args, **kwargs)
        scope, source = self._context()
        self.ops.append({
            "id": f"o{len(self.ops)}", "op": str(func), "inputs": inputs,
            "outputs": [tensor for tensor in map(self._tensor, _tensors(output))
                        if tensor not in inputs],
            "scope": scope, "source": source,
        })
        self.calls.append((func, args, kwargs))
        return output

    def __enter__(self):
        self.graph_token = self.graph_recorder.set(self)
        return super().__enter__()

    def __exit__(self, *args):
        try:
            return super().__exit__(*args)
        finally:
            self.graph_recorder.reset(self.graph_token)
            self.graph_token = None


def _counts(indices, size):
    flat = (indices.flatten(1) if indices.dim() > 1 else indices[:, None]).long()
    return torch.zeros(len(flat), size, dtype=torch.float64).scatter_add_(
        1, flat, torch.ones_like(flat, dtype=torch.float64))


def _error(expected, actual):
    return float((expected - actual.double()).abs().max().detach()) if actual.numel() else 0.0


def linear_input(values, terms):
    return torch.cat([_counts(values[data], len(values[weight]))[:, start:end]
                      if kind == "onehot" else values[data].double()
                      for kind, data, weight, start, end in terms], 1)


def linear_weight(values, terms):
    return torch.cat([values[weight].double()[start:end] for _, _, weight, start, end in terms])


def _repeats(signature):
    best = None
    for period in range(2, len(signature) // 2 + 1):
        for start in range(len(signature) - 2 * period + 1):
            count = 1
            while signature[start + count * period:start + (count + 1) * period] \
                    == signature[start:start + period]:
                count += 1
            if count > 1 and (best is None or count * period > best[1] * best[2]):
                best = start, period, count
    return best if best and 2 * best[1] * best[2] >= len(signature) else None


def semantic_graph(recorder, outputs):
    ops, tensors, values, names = recorder.ops, recorder.tensors, recorder.values, recorder.names
    producer, consumers = {}, defaultdict(list)
    for index, op in enumerate(ops):
        for tensor in op["inputs"]:
            consumers[tensor].append(index)
        for tensor in op["outputs"]:
            producer.setdefault(tensor, index)

    learned, observed = {}, {}
    for tensor in tensors:
        if tensor not in producer:
            learned[tensor] = id(values[tensor]) in recorder.parameters
            observed[tensor] = not learned[tensor]
    for op in ops:
        base = _base(op)
        constant = base.startswith("new_") or base.endswith("_like")
        for tensor in op["outputs"]:
            learned[tensor] = not constant and any(learned[t] for t in op["inputs"])
            observed[tensor] = not constant and any(observed[t] for t in op["inputs"])
    origins = {
        tensor: ({(True, True): "activation", (True, False): "parameter",
                  (False, True): "input", (False, False): "constant"}
                 if values[tensor].is_floating_point() else
                 {(False, False): "constant"}).get((learned[tensor], observed[tensor]), "index")
        for tensor in tensors
    }

    def visible(op):
        base = _base(op)
        if not any(origins[tensor] == "activation" for tensor in op["outputs"]):
            return False
        if base in GATHERS:
            return base == "embedding" or origins[op["inputs"][0]] == "parameter"
        return base not in PLUMBING

    roots_memo = {}
    def roots(tensor):
        if tensor not in roots_memo:
            roots_memo[tensor] = {recorder.parameters[id(values[tensor])]} \
                if tensor not in producer and id(values[tensor]) in recorder.parameters else \
                set().union(*(roots(t) for t in ops[producer[tensor]]["inputs"]
                              if origins[t] == "parameter")) if tensor in producer else set()
        return roots_memo[tensor]

    def named(tensor):
        queue, seen = [tensor], set()
        while queue:
            tensor = queue.pop(0)
            if tensor in names:
                return names[tensor]
            if tensor in producer and tensor not in seen:
                seen.add(tensor)
                queue += [t for t in ops[producer[tensor]]["inputs"] if origins[t] != "constant"]
        return None

    def parameter_label(parameters, module=None):
        stems = sorted({name.removesuffix("weight").removesuffix("bias").rstrip("._")
                        for name in parameters})
        name = os.path.commonprefix(stems).rstrip("._") or ", ".join(stems)
        if module and name.startswith(module + "."):
            name = name[len(module) + 1:]
        return _humanize(name)

    def module_of(scope):
        while scope != "root" and "module" not in recorder.scopes[scope]:
            scope = recorder.scopes[scope]["parent"]
        return recorder.scopes[scope]["module"] if scope != "root" else None

    sources, nodes, edges, leaf_edges, hidden = {}, {}, [], [], {}
    def source(tensor):
        if tensor in sources:
            return sources[tensor]
        return {"leaf:" + tensor: (frozenset(), tensor)} \
            if origins[tensor] in ("parameter", "input") else {}

    for index, op in enumerate(ops):
        base = _base(op)
        if not visible(op):
            if any(origins[tensor] == "activation" for tensor in op["outputs"]):
                merged = {}
                for tensor in op["inputs"]:
                    for key, (via, start) in source(tensor).items():
                        merged[key] = (merged.get(key, (frozenset(),))[0] | via | {base}, start)
                for tensor in op["outputs"]:
                    sources[tensor] = merged
            hidden[op["id"]] = "plumbing" if any(
                origins[tensor] == "activation" for tensor in op["outputs"]
            ) else origins[op["outputs"][0]] if op["outputs"] else "no output"
            continue
        for tensor in op["outputs"]:
            sources[tensor] = {op["id"]: (frozenset(), tensor)}
        parameters = sorted(set().union(*(roots(tensor) for tensor in op["inputs"]
                                          if origins[tensor] == "parameter")))
        kind = "Attention" if "scaled_dot_product" in base else KINDS.get(base, _humanize(base))
        kind = "MatMul" if kind == "Linear" and not parameters else kind
        label = names.get(op["outputs"][0]) or kind in PARAMETRIC and parameters and \
            parameter_label(parameters, module_of(op["scope"])) or kind
        nodes[op["id"]] = {
            "id": op["id"], "parent": op["scope"], "kind": kind, "label": _humanize(label),
            "code": op["op"].removeprefix("aten."), "op_ids": [op["id"]], "order": index,
            "parameters": parameters,
            "inputs": [t for t in op["inputs"] if origins[t] in ("activation", "input")
                       or kind == "Embedding" and origins[t] == "index"],
            "outputs": [t for t in op["outputs"] if origins[t] == "activation"],
        }
        for position, tensor in enumerate(op["inputs"]):
            if origins[tensor] == "activation":
                for key, (via, start) in source(tensor).items():
                    (leaf_edges if key.startswith("leaf:") else edges).append(
                        {"from": key, "to": op["id"], "via": sorted(via), "tensor": start,
                         "input": tensor, "op": index})
            elif origins[tensor] == "input" or origins[tensor] == "parameter" \
                    and kind not in PARAMETRIC:
                leaf_edges.append({"from": "leaf:" + tensor, "to": op["id"], "via": [],
                                   "tensor": tensor, "input": tensor, "op": index})
            elif origins[tensor] == "index" and kind == "Embedding" and position == 1 \
                    and named(tensor):
                leaf_edges.append({"from": "leaf:" + tensor, "to": op["id"], "via": [],
                                   "tensor": tensor, "input": tensor, "op": index})

    alias, rewrites, rejected = {}, [], []
    def resolve(key):
        while key in alias:
            key = alias[key]
        return key

    def merge(target, others, outputs):
        internal = {t for node in (target, *others) for t in node["outputs"]}
        for other in others:
            target["op_ids"] += other["op_ids"]
            target["parameters"] = sorted(set(target["parameters"]) | set(other["parameters"]))
            target["inputs"] += other["inputs"]
            alias[other["id"]] = target["id"]
            del nodes[other["id"]]
        target["inputs"] = [t for t in dict.fromkeys(target["inputs"]) if t not in internal]
        target["op_ids"].sort(key=lambda op_id: int(op_id[1:]))
        target["outputs"] = outputs

    def direct(tensor):
        keys = [resolve(key) for key in source(tensor)]
        return nodes.get(keys[0]) if len(keys) == 1 else None

    def blocks(table):
        concatenation = producer.get(table)
        if concatenation is None or _base(ops[concatenation]) != "cat" \
                or recorder.calls[concatenation][1][1:2] not in ((), (0,)):
            return [(0, tensors[table]["shape"][0])]
        parts, start = [], 0
        for part in ops[concatenation]["inputs"]:
            end, multiply = start + tensors[part]["shape"][0], producer.get(part)
            if multiply is None or _base(ops[multiply]) != "mul" or not any(
                    type(argument) in (int, float) and argument == 0
                    for argument in recorder.calls[multiply][1][1:]):
                parts.append((start, end))
            start = end
        return parts

    def terms(node):
        op = ops[int(node["id"][1:])]
        if _base(op) == "mm" and origins[op["inputs"][1]] == "parameter":
            return [("dense", op["inputs"][0], op["inputs"][1], 0, None)]
        return node.get("terms")

    def rewrite(target, others, combined, output, identity):
        error = _error(linear_input(values, combined) @ linear_weight(values, combined),
                       values[output])
        record = {"node": target["id"], "kind": "linear", "label": identity,
                  "maximum_error": error}
        if error > 1e-4:
            rejected.append(record)
            return
        merge(target, others, [output])
        target["terms"] = combined
        rewrites.append(record)

    for node in list(nodes.values()):
        op = ops[int(node["id"][1:])]
        if node["kind"] != "Embedding" or origins[op["inputs"][1]] != "index":
            continue
        table, ids = op["inputs"][:2]
        output = op["outputs"][0]
        counts = [("onehot", ids, table, start, end) for start, end in blocks(table)]
        reduction = ops[consumers[output][0]] if len(consumers[output]) == 1 else None
        arguments = recorder.calls[int(reduction["id"][1:])][1] if reduction else ()
        if reduction and _base(reduction) == "sum" and reduction["id"] in nodes \
                and len(arguments) > 1 and list(arguments[1]) == [1]:
            rewrite(node, [nodes[reduction["id"]]], counts, reduction["outputs"][0],
                    "Σₖ table[idₖ] = Σ_c one-hot_c(ids) · table_c, one block per concatenated table")
        elif len(tensors[output]["shape"]) == 2:
            rewrite(node, [], counts, output, "table[id] = Σ_c one-hot_c(id) · table_c")
    for node in list(nodes.values()):
        op, index = ops[int(node["id"][1:])], int(node["id"][1:])
        if _base(op) != "add" or len(op["inputs"]) != 2:
            continue
        zero = [t for t in op["inputs"] if origins[t] == "constant" and t in producer
                and _base(ops[producer[t]]) in ("new_zeros", "zeros", "zeros_like")]
        parts = [direct(t) for t in op["inputs"] if t not in zero]
        if len(parts) + len(zero) != 2 or any(part is None or terms(part) is None
                                              or any(consumers[t] != [index] for t in part["outputs"])
                                              for part in parts):
            continue
        combined = [term for part in parts for term in terms(part)]
        if any(kind == "onehot" for kind, *_ in combined):
            rewrite(node, parts, combined, op["outputs"][0],
                    "x·W₁ + y·W₂ = [x, y] · [W₁; W₂]" if len(parts) == 2 else "x·W + 0 = x·W")

    for node in nodes.values():
        if not node.get("terms"):
            continue
        codes = {term[1]: set(values[term[1]].flatten().tolist()) for term in node["terms"]}
        node["terms"] = [term for term in node["terms"] if term[0] != "onehot"
                         or any(term[3] <= code < term[4] for code in codes[term[1]])] \
            or node["terms"][:1]
        record = next(row for row in rewrites if row["node"] == node["id"])
        record["maximum_error"] = max(record["maximum_error"], _error(
            linear_input(values, node["terms"]) @ linear_weight(values, node["terms"]),
            values[node["outputs"][0]]))
        data = list(dict.fromkeys(term[1] for term in node["terms"]))
        input_names = [named(tensor) for tensor in data]
        words = [name.split() for name in input_names if name]
        prefix = []
        for column in zip(*words):
            if len(set(column)) > 1:
                break
            prefix.append(column[0])
        virtual = f"v{len(recorder.virtual)}"
        values[virtual] = linear_input(values, node["terms"]).float()
        tensors[virtual] = {"id": virtual, "shape": list(values[virtual].shape),
                            "dtype": "torch.float32"}
        origins[virtual] = "input"
        recorder.virtual[virtual] = {
            "terms": node["terms"], "output": node["outputs"][0],
            "code": " ⊕ ".join(f"one-hot({name or 'IDs'})" if tensors[tensor]["dtype"] != "torch.float32"
                               else name or "x" for tensor, name in zip(data, input_names))}
        single = input_names[0] or names.get(node["outputs"][0]) if len(data) == 1 else None
        names[virtual] = " ".join(prefix) + " state vector" if len(words) > 1 and prefix \
            else f"{single} one-hot" if single else " ⊕ ".join(name or "one-hot" for name in input_names)
        node.update(kind="Linear", label="Linear", weight=virtual,
                    code=" ⊕ ".join(dict.fromkeys(("one-hot · " if kind == "onehot" else "x · ")
                                                  + parameter_label(sorted(roots(weight)))
                                                  for kind, _, weight, _, _ in node["terms"])),
                    inputs=[virtual] + [t for t in node["inputs"] if t not in data])
        leaf_edges[:] = [edge for edge in leaf_edges
                         if resolve(edge["to"]) != node["id"] or edge["input"] not in data]
        leaf_edges.append({"from": "leaf:" + virtual, "to": node["id"], "via": [],
                           "tensor": virtual, "input": virtual, "op": int(node["id"][1:])})

    for node in sorted(nodes.values(), key=lambda node: node["order"]):
        output = node["outputs"][0] if node["outputs"] else None
        readers = [index for index in consumers[output] if not _base(ops[index]).startswith("new_")
                   and not _base(ops[index]).endswith("_like")]
        if node["id"] not in nodes or node["kind"] != "Add" or output in names or len(readers) != 1:
            continue
        target = nodes.get(resolve(ops[readers[0]]["id"]))
        if target and target is not node and target["kind"] == "Add":
            merge(target, [node], target["outputs"])

    edges = list({(resolve(edge["from"]), resolve(edge["to"])): edge | {
        "from": resolve(edge["from"]), "to": resolve(edge["to"])} for edge in edges
        if resolve(edge["from"]) != resolve(edge["to"])
        and values[edge["tensor"]].numel() and values[edge["input"]].numel()}.values())
    leaf_edges = [edge | {"to": resolve(edge["to"])} for edge in leaf_edges]
    tree = {"root": {"id": "root", "parent": None, "label": type(recorder.model).__name__,
                     "kind": "model", "code": ""}}
    tree |= {key: dict(scope) for key, scope in recorder.scopes.items()} | nodes

    def children():
        result = defaultdict(list)
        for node in sorted(tree.values(), key=lambda node: node.get("order", 0)):
            if node["parent"]:
                result[node["parent"]].append(node["id"])
        return result

    def reorder():
        for node in tree.values():
            if "op_ids" not in node:
                node["order"] = math.inf
        for node in nodes.values():
            parent = node["parent"]
            while parent:
                tree[parent]["order"] = min(tree[parent]["order"], node["order"])
                parent = tree[parent]["parent"]
        for key in [key for key, node in tree.items() if node["order"] == math.inf]:
            del tree[key]

    def collapse():
        while True:
            key, items = next(((key, items) for key, items in children().items()
                               if key != "root" and len(items) == 1), (None, None))
            if key is None:
                return
            scope, child = tree.pop(key), tree[items[0]]
            child["parent"] = scope["parent"]
            if not child.get("named") and ("op_ids" in child and (
                    "module" in scope or scope.get("qualified"))
                    or child.get("kind") == "function" and not child.get("qualified")):
                child.update(label=scope["label"], code=scope["code"])
                child["qualified"] = scope.get("qualified")
            elif "op_ids" in child:
                child["code"] = scope["code"]

    reorder()
    collapse()
    for key, items in list(children().items()):
        found = _repeats([(tree[item]["kind"], tree[item]["label"]) for item in items])
        if found:
            start, period, count = found
            for step in range(count):
                group = f"{key}.{step}"
                tree[group] = {"id": group, "parent": key, "kind": "repeat",
                               "label": f"{tree[key]['label']} · step {step}",
                               "code": tree[key]["code"]}
                for item in items[start + step * period:start + (step + 1) * period]:
                    tree[item]["parent"] = group

    def ancestor(key, scope):
        while key in tree and tree[key]["parent"] != scope:
            key = tree[key]["parent"]
            if key is None:
                return None
        return key if key in tree else None

    named_nodes = [(name, keys[0]) for tensor, name in names.items()
                   if len(keys := [resolve(key) for key in sources.get(tensor, {})]) == 1
                   and keys[0] in nodes]
    reorder()
    for scope, items in list(children().items()):
        successors, exits, producers = defaultdict(set), set(), {}
        for edge in edges:
            left, right = ancestor(edge["from"], scope), ancestor(edge["to"], scope)
            if left and right and left != right:
                successors[left].add(right)
            elif left and not right:
                exits.add(left)
        for name, key in named_nodes:
            item = ancestor(key, scope)
            if not item or item in producers.values():
                continue
            leaving = {edge["from"] for edge in edges if (edge["from"] == item or ancestor(
                edge["from"], item)) and edge["to"] != item and not ancestor(edge["to"], item)}
            if leaving == {key}:
                producers[name] = item
        if not producers:
            continue
        memo = {}
        def destinations(item):
            if item not in memo:
                memo[item] = set()
                memo[item] = {item} if item in producers.values() else set().union(
                    *(destinations(successor) for successor in successors[item])
                ) | ({None} if item in exits else set())
            return memo[item]
        for name, item in producers.items():
            members = [other for other in items
                       if other not in producers.values() and destinations(other) == {item}]
            if len(members) + 1 == len(items):
                continue
            if not members:
                tree[item].update(label=_humanize(name), qualified=True, named=True)
                continue
            group = f"{scope}:{name}"
            tree[group] = {"id": group, "parent": scope, "kind": "group", "named": True,
                           "label": _humanize(name), "code": tree[item]["label"]}
            for member in members + [item]:
                tree[member]["parent"] = group
    reorder()
    collapse()
    reorder()

    def argument(index, tensor):
        func, args, kwargs = recorder.calls[index]
        named_arguments = [*zip((schema.name for schema in func._schema.arguments), args),
                           *kwargs.items()]
        return next((name for name, value in named_arguments if isinstance(value, torch.Tensor)
                     and recorder.tensor_ids.get(id(value)) == tensor), "input")

    leaves = {}
    for edge in leaf_edges:
        tensor = edge["from"][5:]
        key = f"{edge['from']}@{tree[edge['to']]['parent']}"
        if not values[tensor].numel():
            continue
        if key not in leaves:
            parameters = sorted(roots(tensor))
            label = names.get(tensor) or named(tensor) or (parameter_label(parameters)
                if parameters else argument(edge["op"], edge["input"]))
            leaves[key] = tree[key] = {
                "id": key, "parent": tree[edge["to"]]["parent"], "leaf": origins[tensor],
                "kind": "Parameter" if origins[tensor] == "parameter" else "Input",
                "label": _humanize(label), "code": ", ".join(parameters) or recorder.virtual.get(
                    tensor, {}).get("code") or "model input",
                "order": tree[edge["to"]]["order"] - .5, "outputs": [tensor],
                "inputs": [], "parameters": parameters,
            }
        edges.append(edge | {"from": key})

    items = children()
    descendants = {}
    def atoms(key):
        if key not in descendants:
            descendants[key] = [key] if "op_ids" in tree[key] or tree[key].get("leaf") else \
                [atom for item in items[key] for atom in atoms(item)]
        return descendants[key]
    atoms("root")
    predecessors, successors = defaultdict(set), defaultdict(set)
    for edge in edges:
        if edge["from"] in nodes:
            predecessors[edge["to"]].add(edge["from"])
            successors[edge["from"]].add(edge["to"])
    before, after, ordered = {}, {}, sorted(nodes, key=lambda key: nodes[key]["order"])
    for key in ordered:
        before[key] = any(nodes[other]["kind"] == "Attention" or before[other]
                          for other in predecessors[key])
    for key in reversed(ordered):
        after[key] = any(nodes[other]["kind"] == "Attention" or after[other]
                         for other in successors[key])
    head = {key: before[key] and not after[key] for key in nodes}
    for key, node in tree.items():
        inside = set(atoms(key))
        entering = [edge for edge in edges if edge["to"] in inside and edge["from"] not in inside]
        leaving = [edge for edge in edges if edge["from"] in inside and edge["to"] not in inside]
        if "op_ids" not in node:
            node["inputs"] = list(dict.fromkeys(edge["input"] for edge in entering
                                                if not edge["from"].startswith("leaf:")))[:8]
            node["outputs"] = list(dict.fromkeys(
                [edge["tensor"] for edge in leaving] + [tensor for tensor in outputs if any(
                    resolve(source_key) in inside for source_key in sources.get(tensor, {}))]))[:8]
        node["size"] = sum(atom in nodes for atom in inside)
        node["role"] = "input" if node.get("leaf") else "global" if any(
            tree[atom]["kind"] == "Attention" for atom in inside) else "head" if all(
            head.get(atom, False) for atom in inside if atom in nodes) else "input" if not any(
            edge["from"] in nodes for edge in entering) else "local"
        if node.get("outputs"):
            node["shape"] = tensors[node["outputs"][0]]["shape"]

    inline, frontier = [], [key for key in items["root"] if not tree[key].get("leaf")]
    while True:
        options = [key for key in frontier if items.get(key) and not tree[key].get("named")
                   and tree[key]["kind"] != "repeat"
                   and len(frontier) - 1 + len(items[key]) <= NODE_BUDGET]
        if not options:
            break
        key = max(options, key=lambda key: tree[key]["size"])
        inline.append(key)
        index = frontier.index(key)
        frontier[index:index + 1] = [item for item in items[key] if not tree[item].get("leaf")]

    covered = {op_id for node in nodes.values() for op_id in node["op_ids"]}
    if covered & hidden.keys() or covered | hidden.keys() != {op["id"] for op in ops}:
        raise RuntimeError("model graph trace has incomplete operation coverage")
    return {
        "schema": TRACE_SCHEMA,
        "nodes": sorted(tree.values(), key=lambda node: node["order"]),
        "edges": [{key: edge[key] for key in ("from", "to", "via", "tensor", "input")}
                  for edge in edges],
        "inline": inline,
        "execution": sorted(nodes, key=lambda key: nodes[key]["order"]),
        "hidden": hidden,
        "raw_ops": ops,
        "tensors": {key: tensors[key] | {"origin": origins[key]} for key in tensors},
        "outputs": outputs,
        "rewrites": rewrites,
        "rejected": rejected,
    }


def label_tensors(recorder, rows=lambda name, count: None, channels=lambda name, count: None):
    ops, tensors, values, table, codes = recorder.ops, recorder.tensors, recorder.values, [], {}
    labels = {"rows": recorder.labels, "channels": recorder.channels}
    produced = {tensor: index for index, op in enumerate(ops) for tensor in op["outputs"]}
    learned = {tensor for tensor in tensors if tensor not in produced
               and id(values[tensor]) in recorder.parameters}
    for op in ops:
        if op["inputs"] and all(tensor in learned for tensor in op["inputs"]):
            learned.update(op["outputs"])

    def size(tensor, axis):
        shape = tensors[tensor]["shape"]
        return math.prod(shape[:-1]) if axis == "rows" else shape[-1]

    def floating(tensor):
        return values[tensor].is_floating_point() and values[tensor].dim() > 0

    def fill(tensor, axis, update):
        current = labels[axis].setdefault(tensor, [None] * size(tensor, axis))
        for position, label in update:
            if 0 <= position < len(current) and current[position] is None and label is not None:
                current[position] = label

    def complete(tensor, axis):
        return tensor in labels[axis] and None not in labels[axis][tensor]

    def encode(tensor, axis, identity):
        shape = tensors[tensor]["shape"]
        ids = torch.full((size(tensor, axis),), -1, dtype=torch.float64)
        if tensor == identity:
            ids = torch.arange(len(ids), dtype=torch.float64)
        elif identity is None:
            for position, label in enumerate(labels[axis].get(tensor, ())):
                if label is not None:
                    if label not in codes:
                        codes[label] = len(table); table.append(label)
                    ids[position] = codes[label]
        view = shape[:-1] + [1] if axis == "rows" else [1] * (len(shape) - 1) + shape[-1:]
        return ids.reshape(view).expand(shape).clone()

    def replay(index, axis, identity=None):
        func, args, kwargs = recorder.calls[index]
        def swap(value):
            if isinstance(value, torch.Tensor):
                tensor = recorder.tensor_ids.get(id(value))
                return encode(tensor, axis, identity) if tensor and floating(tensor) else value
            return type(value)(map(swap, value)) if isinstance(value, (list, tuple)) else value
        try:
            output = next(_tensors(func(*swap(args), **{k: swap(v) for k, v in kwargs.items()})))
        except (RuntimeError, TypeError, IndexError, StopIteration):
            return None
        if output.dim() == 0 or not output.numel():
            return None
        flat = output.reshape(-1, output.shape[-1])
        return (flat[:, 0] if axis == "rows" else flat[0]).long().tolist()

    def seed(names, axis):
        for tensor, name in recorder.names.items():
            if floating(tensor):
                fill(tensor, axis, enumerate(names(name, size(tensor, axis)) or ()))

    def propagate_rows():
        for forward in (True, False, True):
            for index in range(len(ops)) if forward else reversed(range(len(ops))):
                op, base = ops[index], _base(ops[index])
                outputs = [t for t in op["outputs"] if floating(t)]
                inputs = [t for t in op["inputs"] if floating(t)]
                if not outputs or base in AGGREGATES and not forward:
                    continue
                output, structural = outputs[0], base in PLUMBING or base in AGGREGATES
                if forward and not complete(output, "rows"):
                    if base in AGGREGATES:
                        fill(output, "rows", enumerate(labels["rows"].get(inputs[0], ())))
                    elif structural:
                        ids = replay(index, "rows")
                        if ids is not None and len(ids) == size(output, "rows"):
                            fill(output, "rows", ((row, table[i] if i >= 0 else None)
                                                  for row, i in enumerate(ids)))
                    elif match := next((t for t in inputs if t in labels["rows"] and
                                        tensors[t]["shape"][:-1] == tensors[output]["shape"][:-1]),
                                       None):
                        fill(output, "rows", enumerate(labels["rows"][match]))
                elif not forward and output in labels["rows"]:
                    for tensor in inputs:
                        if complete(tensor, "rows") or tensor in learned and base in GATHERS:
                            continue
                        if structural:
                            ids = replay(index, "rows", tensor)
                            if ids is not None and len(ids) == size(output, "rows"):
                                fill(tensor, "rows", ((i, labels["rows"][output][row])
                                                      for row, i in enumerate(ids) if i >= 0))
                        elif tensors[tensor]["shape"][:-1] == tensors[output]["shape"][:-1]:
                            fill(tensor, "rows", enumerate(labels["rows"][output]))

    seed(rows, "rows")
    propagate_rows()
    for tensor, name in recorder.names.items():
        if floating(tensor) and tensor in produced:
            count = size(tensor, "rows")
            fill(tensor, "rows", ((row, _humanize(name) + (f" {row + 1}" if count > 1 else ""))
                                  for row in range(count)))
    propagate_rows()

    seed(channels, "channels")
    for index, op in enumerate(ops):
        base = _base(op)
        outputs = [t for t in op["outputs"] if floating(t)]
        inputs = [t for t in op["inputs"] if floating(t)]
        if base in ("mm", "addmm"):
            data, weight = inputs[-2:]
            source = ops[produced[weight]] if weight in produced else None
            if source and _base(source) in ("t", "transpose"):
                weight = source["inputs"][0]
            if data in labels["channels"] and size(weight, "channels") == size(data, "channels"):
                fill(weight, "channels", enumerate(labels["channels"][data]))
        if base in GATHERS and op["inputs"][0] in labels["rows"] and len(op["inputs"]) > 1:
            recorder.vocabularies[op["inputs"][1]] = op["inputs"][0]
        if not outputs or complete(outputs[0], "channels"):
            continue
        if base in PLUMBING or base in AGGREGATES:
            ids = replay(index, "channels")
            if ids is not None and len(ids) == size(outputs[0], "channels"):
                fill(outputs[0], "channels", ((channel, table[i] if i >= 0 else None)
                                              for channel, i in enumerate(ids)))
        elif base in CHANNELWISE and (match := next(
                (t for t in inputs if t in labels["channels"]
                 and size(t, "channels") == size(outputs[0], "channels")), None)):
            fill(outputs[0], "channels", enumerate(labels["channels"][match]))
    for virtual, spec in recorder.virtual.items():
        labels["channels"][virtual] = [
            name for kind, data, weight, start, end in spec["terms"]
            for name in ((labels["rows"].get(weight) or [None] * size(weight, "rows"))[start:end]
                         if kind == "onehot" else
                         labels["channels"].get(data) or [None] * size(data, "channels"))]
        labels["rows"][virtual] = labels["rows"].get(spec["output"])


def trace(model, inputs, function=None):
    recorder, fastpath = OperationRecorder(model), torch.backends.mha.get_fastpath_enabled()
    torch.backends.mha.set_fastpath_enabled(False)
    try:
        with torch.no_grad(), recorder:
            traced = torch.func.functionalize(function or model)(*inputs)
    finally:
        torch.backends.mha.set_fastpath_enabled(fastpath)
    graph = semantic_graph(recorder, [recorder._tensor(value) for value in _tensors(traced)])
    return traced, recorder, graph


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
        traced, recorder, graph = trace(model, inputs[:6], forward)
    finally:
        model._trace_graph = False
        model._graph_cache.clear()

    trace_error = max(float((left - right).abs().max())
                      for left, right in zip(normal, traced))
    if any(not torch.equal(left, right) for left, right in zip(normal, uncached)) \
            or trace_error > 1e-6:
        raise RuntimeError("model graph trace does not reproduce ordinary inference")
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
        "semantic_nodes": len(graph["execution"]),
        "hidden_operations": len(graph["hidden"]),
        "operation_coverage": 1.0,
        "verified_rewrites": len(graph["rewrites"]),
        "rejected_rewrites": len(graph["rejected"]),
        "maximum_rewrite_error": max((row["maximum_error"] for row in graph["rewrites"]),
                                     default=0.0),
        "maximum_output_error": trace_error,
        "output_sha256": [_hash_tensor(value) for value in normal],
        "traced_output_sha256": [_hash_tensor(value) for value in traced],
        "proof_target": "cpu/fp32",
    }
    proof["attestation_sha256"] = hashlib.sha256(json.dumps(
        proof, sort_keys=True, separators=(",", ":"),
    ).encode()).hexdigest()
    graph_id = hashlib.sha256(json.dumps({
        "proof": proof, "nodes": graph["nodes"], "edges": graph["edges"],
    }, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"graph_id": graph_id, "graph": graph, "proof": proof}, recorder


def tensor_detail(value, row_labels=None, limit=1 << 18, channels=None, vocabulary=None):
    codes = value.detach().cpu() if vocabulary and not value.is_floating_point() else None
    value = value.detach().cpu().float().contiguous()
    summary = {"shape": list(value.shape), "dtype": str(value.dtype)}
    selected = value[torch.isfinite(value)]
    if len(selected):
        summary |= {
            "mean": float(selected.mean()), "stddev": float(selected.std(unbiased=False)),
            "rms": float(selected.square().mean().sqrt()), "minimum": float(selected.min()),
            "maximum": float(selected.max()),
        }
    if value.numel() <= limit:
        summary["data"] = base64.b64encode(value.numpy().tobytes()).decode()
    if row_labels and any(row_labels):
        summary["row_labels"] = row_labels
    if channels and any(channels):
        summary["channel_labels"] = channels
    if codes is not None and codes.dim():
        summary["decoded"] = [[vocabulary[code] for code in row if code]
                              for row in codes.reshape(len(codes), -1)[:256].tolist()]
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
