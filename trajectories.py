import gzip
import json
import math
import os
import shutil
import threading
import time
import uuid
from pathlib import Path

import torch

import sts2_sim
from explanations import explain as explain_decision
from model import CATEGORIES, MAX_PROGRESS, MODEL_VERSION, POTENTIAL_TERMS, STAGES, \
    TOKEN_SPECS, export_value_model, load, potential_head, predict, sha256_file, tensors
from model_graph import GraphCache, NUMERIC_CHANNELS, feature_schema, label_tensors, \
    linear_weight, tensor_detail, trace_model


BOSS_FLOOR_INCREMENTS = (10., 11., 10.)


def _logs(run):
    paths = []
    for name in ("events.jsonl", "events.jsonl.gz"):
        path = run / name
        if path.exists():
            paths.append(path)
    directory = run / "events"
    if directory.exists():
        logs = {path.name.removesuffix(".gz"): path
                for pattern in ("*.jsonl", "*.jsonl.gz") for path in directory.glob(pattern)}
        paths += [logs[name] for name in sorted(logs)]
    return paths


def _open(path, mode="rt"):
    return gzip.open(path, mode) if path.suffix == ".gz" else path.open(mode)


def _metadata(trace):
    return {key: trace.get(key) for key in (
        "id", "source", "time", "session_id", "worker", "generation", "seed",
        "character", "stage", "ascension", "bonus", "temperature", "sampled",
        "checkpoint", "outcome",
        "terminal", "terminal_floor", "completion_seconds", "started_iteration",
        "completed_iteration",
    )} | {
        "length": len(trace.get("choices", ())),
        "policy_revision_min": min(trace.get("policy_revisions", ()), default=None),
        "policy_revision_max": max(trace.get("policy_revisions", ()), default=None),
    }


def validate_trace(trace):
    if trace.get("trajectory_schema") not in (1, 2):
        raise ValueError("unsupported trajectory schema")
    choices = trace.get("choices")
    fields = ("log_policies", "critic_values", "canonical_progress", "phases",
              "policy_revisions")
    if not isinstance(choices, list) or any(len(trace.get(field, ())) != len(choices)
                                            for field in fields):
        raise ValueError("invalid trajectory fields")
    for choice, policy in zip(choices, trace["log_policies"]):
        values = [float(value) for value in policy] if isinstance(policy, list) else []
        if not values or not 0 <= int(choice) < len(values) \
                or any(not math.isfinite(value) for value in values) \
                or not math.isclose(sum(math.exp(value) for value in values), 1, abs_tol=1e-4):
            raise ValueError("invalid trajectory policy")
    if any(not math.isfinite(float(value)) for value in trace["critic_values"]):
        raise ValueError("invalid trajectory values")
    for key in ("seed", "character", "ascension", "bonus", "fingerprint"):
        int(trace[key])
    return trace


def _training_config(manifest, trace):
    sessions = manifest.get("sessions", ())
    session_id = trace.get("session_id")
    session = next((row for row in sessions
                    if session_id is not None and str(row.get("id")) == str(session_id)), None)
    training = (session or sessions[-1]).get("training", {}) if sessions \
        else manifest.get("training", {})
    return {
        "gae_gamma": float(training.get("gae_gamma", 1)),
        "gae_lambda": float(training.get("gae_lambda", 1)),
        "clip": float(training.get("clip", .2)),
        "value_weight": float(training.get("value_weight", .5)),
        "entropy_weight": (float(training["entropy_weight"])
                           if training.get("entropy_weight") is not None else None),
        "boss_floor_increments": [float(training.get(
            f"boss_{boss}_floor_increment", default,
        )) for boss, default in enumerate(BOSS_FLOOR_INCREMENTS, 1)],
        "potential_head": potential_head(
            [float(training.get(f"potential_{term}_weight", 0.)) for term in POTENTIAL_TERMS],
            training.get("potential_mode", "health_multiplier"),
        ),
        "session_id": session.get("id") if session else sessions[-1].get("id")
        if sessions else None,
    }


def _terminal_value(trace, config):
    won = trace.get("outcome") == "won"
    category = CATEGORIES - 1 if won else max(map(int, trace["canonical_progress"]), default=0)
    increments = config["boss_floor_increments"]
    value = MAX_PROGRESS - sum(BOSS_FLOOR_INCREMENTS[:2]) + sum(increments) if won else \
        category + sum(increment - default for threshold, increment, default in zip(
            (25, 54), increments[:2], BOSS_FLOOR_INCREMENTS[:2],
        ) if category >= threshold)
    return value / (CATEGORIES - 1)


def _add_training_signals(trace, steps, config):
    terminal = bool(trace.get("terminal"))
    actionable = [index for index, step in enumerate(steps) if len(step["actions"]) > 1]
    values = {}
    if terminal and actionable:
        gamma, gae_lambda = config["gae_gamma"], config["gae_lambda"]
        terminal_value = _terminal_value(trace, config)
        gae = discounted_return = next_value = 0.
        for position in range(len(actionable) - 1, -1, -1):
            index = actionable[position]
            step = steps[index]
            following_return, following_advantage = discounted_return, gae
            reward = terminal_value if position + 1 == len(actionable) else 0.
            delta = reward + gamma * next_value - step["critic_value"]
            gae = delta + gamma * gae_lambda * gae
            discounted_return = reward + gamma * discounted_return
            values[index] = {
                "reward": reward,
                "next_actionable_index": actionable[position + 1]
                if position + 1 < len(actionable) else None,
                "next_critic_value": next_value,
                "next_return": following_return, "next_advantage": following_advantage,
                "return": discounted_return, "td_error": delta, "advantage": gae,
                "critic_error": step["critic_value"] - discounted_return,
                "critic_squared_error": (step["critic_value"] - discounted_return) ** 2,
            }
            next_value = step["critic_value"]
    for index, step in enumerate(steps):
        eligible = terminal and index in values
        reason = None if eligible else (
            "This trajectory did not reach a terminal state and would be discarded."
            if not terminal else "This forced choice would be omitted from the training dataset."
        )
        step["training_signal"] = {
            "eligible": eligible, "reason": reason,
            "critic_value": step["critic_value"], "potential": step["potential"],
            "network_value": step["critic_value"] - step["potential"],
            "entropy": step["policy_entropy"],
            "selected_log_probability": step["actions"][step["choice"]]["log_probability"],
            "potential_components": step["potential_components"],
        } | values.get(index, {})


def replay_trace(trace, config=None):
    validate_trace(trace)
    config = config or _training_config({}, trace)
    env = sts2_sim.Batch(
        1, int(trace["seed"]), int(trace["character"]),
        ascension=int(trace["ascension"]),
    )
    env.set_training_bonus(int(trace["bonus"]))
    if env.fingerprint() != int(trace["fingerprint"]):
        raise ValueError("trajectory does not match simulator content")
    resources = POTENTIAL_TERMS[1:]
    names = ["floor", "health", *resources, *(f"health_{name}" for name in resources)]
    steps = []
    for index, choice in enumerate(trace["choices"]):
        features = env.observe_tokens()[1][0].tolist()
        state = json.loads(env.state(0))
        potential_components = [
            {"name": name, "weight": weight, "feature": feature, "contribution": weight * feature}
            for name, weight, feature in zip(names, config["potential_head"], features)
        ]
        potential = sum(row["contribution"] for row in potential_components)
        actions = env.action_descriptors()
        previews = [json.loads(value) if value else None for value in env.action_previews()]
        policy = trace["log_policies"][index]
        if len(actions) != len(policy) or len(previews) != len(actions) \
                or not 0 <= choice < len(actions):
            raise ValueError(f"trajectory diverged at decision {index}")
        probabilities = [math.exp(value) for value in policy]
        entropy = -sum(probability * log_probability
                       for probability, log_probability in zip(probabilities, policy))
        steps.append({
            "index": index,
            "state": state,
            "choice": choice,
            "actions": [
                {
                    "index": action_index, "kind": action[0], "source": action[1],
                    "target": action[2], "label": action[3],
                    "log_probability": log_probability,
                    "probability": probability,
                    "preview": preview,
                    "selected": action_index == choice,
                }
                for action_index, (action, log_probability, probability, preview)
                in enumerate(zip(actions, policy, probabilities, previews))
            ],
            "policy_entropy": entropy,
            "effective_actions": math.exp(entropy),
            "critic_value": trace["critic_values"][index]
            + (potential if trace["trajectory_schema"] == 1 else 0.),
            "canonical_progress": trace["canonical_progress"][index],
            "phase": trace["phases"][index],
            "policy_revision": trace["policy_revisions"][index],
            "potential": potential,
            "potential_components": potential_components,
        })
        steps[-1]["done"] = bool(env.step([choice])[1][0])
    _add_training_signals(trace, steps, config)
    return {"trace": trace, "steps": steps, "final_state": json.loads(env.state(0)),
            "training_config": config}


CONTENT_NAMESPACES = {"character", "card", "power", "relic", "potion", "enemy", "enemy_move", "orb",
                      "event", "encounter_normal", "encounter_elite"}


def _title(value):
    prefix, dot, name = value.partition(".")
    return name.replace("_", " ").title() if dot and prefix.isupper() else value


def _readable(concept, namespace=True):
    kind, separator, value = concept.partition(": ")
    if not separator:
        return _title(concept)
    value = _title(value) if kind in CONTENT_NAMESPACES else value
    return f"{kind.replace('_', ' ')}: {value}" if namespace or value.isdigit() else value


def decision_labels(concepts, inputs, actions):
    """Name rows and channels of one decision from the simulator's concept vocabulary."""
    domains = {name: values for (name, *_), values in zip(TOKEN_SPECS, inputs[2])}
    flags = [_readable(concept, False) for concept in concepts if concept.startswith("flag_bit: ")]
    channels = dict(NUMERIC_CHANNELS, card=NUMERIC_CHANNELS["card"] + tuple(flags) + tuple(
        f"{flag} this turn" for flag in flags) + ("free", "cost override set"))
    action_names = [_title(action[3] or action[0]) for action in actions]

    def row(codes):
        names = [concepts[code] for code in codes if code]
        content = next((name for name in names if name.split(": ")[0] in CONTENT_NAMESPACES), None)
        return " · ".join(dict.fromkeys(_readable(name, False) for name in names[:1] + [content]
                                        if name)) or None

    def rows(name, count):
        if name == "concepts":
            return [_readable(concept) for concept in concepts]
        if name in ("action", "action numeric features"):
            return action_names
        domain = name.removesuffix(" numeric features").removesuffix(" encoding")
        if domain == "run":
            return ["run"] * count
        return [row(codes) for codes in domains[domain][0][:count].tolist()] \
            if domain in domains else None

    return rows, lambda name, count: channels.get(name.removesuffix(" numeric features")) \
        if name.endswith(" numeric features") else None


def _checkpoint_file(run, trace):
    snapshot = run / "snapshots" / f"{trace['checkpoint_sha256']}.pt"
    return snapshot if snapshot.is_file() else (run / str(trace["checkpoint"])).resolve()


class TrajectorySource:
    def __init__(self, target):
        self.target = Path(target).resolve()
        self.generated = {}
        self.cache = {}
        self.model = None
        self.explanations = {}
        self.graphs = GraphCache(8)
        self.graph_keys = {}
        self.lock = threading.Lock()

    def _run(self, name):
        run = (self.target / name).resolve()
        if not run.is_relative_to(self.target) or not (run / "run.json").is_file():
            raise ValueError("invalid run")
        return run

    def catalog(self):
        paths = list(self.target.rglob("run.json"))

        def order(path):
            try:
                compatible = json.loads(path.read_text()).get("model_version") == MODEL_VERSION
            except (OSError, ValueError, json.JSONDecodeError):
                compatible = False
            return compatible, path.stat().st_mtime
        return [str(path.parent.relative_to(self.target))
                for path in sorted(paths, key=order, reverse=True)]

    def _captured(self, name):
        run = self._run(name)
        paths = _logs(run)
        signature = tuple((str(path), path.stat().st_size, path.stat().st_mtime_ns)
                          for path in paths)
        if name in self.cache and self.cache[name][0] == signature:
            return self.cache[name][1]
        traces = {}
        for path in paths:
            try:
                with _open(path) as lines:
                    for line in lines:
                        try:
                            trace = json.loads(line)
                            if trace.get("event") == "trajectory":
                                validate_trace(trace)
                                traces[str(trace["id"])] = trace | {"source": "training"}
                        except (KeyError, TypeError, ValueError, json.JSONDecodeError):
                            continue
            except OSError:
                continue
        self.cache[name] = signature, traces
        return traces

    def _generated(self, name):
        if name not in self.generated:
            self.generated[name] = {trace["id"]: trace for trace in (
                json.loads(path.read_text())
                for path in sorted((self._run(name) / "generated").glob("*.json")))}
        return self.generated[name]

    def _remember(self, name, trace):
        folder = self._run(name) / "generated"
        folder.mkdir(exist_ok=True)
        temporary = folder / f".{uuid.uuid4().hex}.tmp"
        temporary.write_text(json.dumps(trace))
        os.replace(temporary, folder / f"{trace['id'].split(':')[1]}.json")
        self._generated(name)[trace["id"]] = trace

    def _checkpoints(self, run):
        paths = [run / "latest.pt", run / "initial.pt"]
        paths += sorted((run / "checkpoints").glob("*.pt"))
        paths += sorted((run / "stage-champions").glob("*.pt"))
        return [str(path.relative_to(run)) for path in paths if path.is_file()]

    def list(self, name):
        with self.lock:
            run = self._run(name)
            manifest = json.loads((run / "run.json").read_text())
            sessions = manifest.get("sessions", [])
            training = sessions[-1].get("training", manifest.get("training", {})) \
                if sessions else manifest.get("training", {})
            compatible = manifest.get("model_version") == MODEL_VERSION
            traces = self._captured(name) | self._generated(name)
            return {
                "run": name,
                "checkpoints": self._checkpoints(run) if compatible else [],
                "checkpoint_error": None if compatible else
                    f"Generation requires model v{MODEL_VERSION}; this run uses "
                    f"v{manifest.get('model_version', 'unknown')}.",
                "defaults": {
                    "seed": training.get("training_seed", 1),
                    "temperature": training.get("policy_temperature", .8),
                    "max_steps": training.get("max_steps", 2048),
                    "max_combat_steps": training.get("max_combat_steps", 512),
                },
                "traces": [_metadata(trace) for trace in sorted(
                    traces.values(), key=lambda trace: (
                        trace.get("time", 0), trace.get("completed_iteration", 0)
                    ), reverse=True,
                )],
            }

    def get(self, name, trace_id):
        with self.lock:
            run = self._run(name)
            trace = self._generated(name).get(trace_id) or self._captured(name).get(trace_id)
            if trace is None:
                raise ValueError("unknown trajectory")
            result = replay_trace(trace, _training_config(json.loads(
                (run / "run.json").read_text()
            ), trace))
            result["model_graph"] = self._graph_availability(run, trace)
            return result

    @staticmethod
    def _graph_availability(run, trace):
        checkpoint, digest = trace.get("checkpoint"), trace.get("checkpoint_sha256")
        if not checkpoint or not digest:
            return {"available": False, "reason": "exact checkpoint proof is unavailable"}
        path = _checkpoint_file(run, trace)
        if not path.is_relative_to(run) or not path.is_file():
            return {"available": False, "reason": "exact checkpoint file is unavailable"}
        if sha256_file(path) != str(digest):
            return {"available": False, "reason": "checkpoint SHA-256 does not match"}
        return {"available": True}

    def model_graph(self, name, request):
        with self.lock:
            if request.get("graph_id"):
                summary, recorder, model = self.graphs.get(str(request["graph_id"]))
                node_id = str(request["node_id"])
                node = next((row for row in summary["graph"]["nodes"]
                             if row["id"] == node_id), None)
                if node is None:
                    raise ValueError("unknown model graph node")
                values = {
                    tensor_id: tensor_detail(
                        recorder.values[tensor_id], recorder.labels.get(tensor_id),
                        channels=recorder.channels.get(tensor_id),
                        vocabulary=recorder.labels.get(recorder.vocabularies.get(tensor_id)))
                    for tensor_id in dict.fromkeys(node["inputs"] + node["outputs"])
                }
                named, parameters, budget = dict(model.named_parameters()), {}, 1 << 16
                if virtual := node.get("weight"):
                    columns = recorder.values[virtual].abs().sum(0).nonzero().flatten()[:256]
                    channels = recorder.channels.get(virtual) or []
                    parameters["weight · active rows"] = tensor_detail(
                        linear_weight(recorder.values, recorder.virtual[virtual]["terms"])[columns],
                        [channels[column] if column < len(channels) and channels[column]
                         else f"channel {column}" for column in columns.tolist()])
                for name in () if node.get("weight") else node.get("parameters", ()):
                    tensor_id = recorder.tensor_ids.get(id(named[name]))
                    parameters[name] = tensor_detail(
                        named[name], recorder.labels.get(tensor_id), budget,
                        recorder.channels.get(tensor_id))
                    budget -= named[name].numel() if "data" in parameters[name] else 0
                return {"graph_id": request["graph_id"], "node": node,
                        "tensors": values, "parameters": parameters}

            run = self._run(name)
            trace_id, step = str(request["trajectory"]), int(request["step"])
            trace = self._generated(name).get(trace_id) or self._captured(name).get(trace_id)
            if trace is None:
                raise ValueError("unknown trajectory; reopen it from the trajectory list")
            if not 0 <= step < len(trace["choices"]):
                raise ValueError("invalid trajectory decision")
            availability = self._graph_availability(run, trace)
            if not availability["available"]:
                raise ValueError(availability["reason"])
            checkpoint_path = _checkpoint_file(run, trace)
            digest = str(trace["checkpoint_sha256"])
            if sha256_file(checkpoint_path) != digest:
                raise ValueError("checkpoint proof does not match the exact file")
            env = sts2_sim.Batch(1, int(trace["seed"]), int(trace["character"]),
                                 ascension=int(trace["ascension"]))
            env.set_training_bonus(int(trace["bonus"]))
            for choice in trace["choices"][:step]:
                env.observe_tokens(); env.step([int(choice)])
            observation = env.observe_tokens()
            cache_key = digest, int(observation[4][0]), step
            graph_id = self.graph_keys.get(cache_key)
            if graph_id:
                try:
                    return self.graphs.get(graph_id)[0]
                except KeyError:
                    self.graph_keys.pop(cache_key, None)
            model, _checkpoint = load(checkpoint_path, torch.device("cpu")); model.eval()
            inputs = tensors(observation, torch.device("cpu"), model)
            temperature = float(trace["temperature"])
            with torch.no_grad():
                policy, critic = predict(model, inputs, "fp32", temperature, flat_policy=True)
            expected_policy = torch.tensor(trace["log_policies"][step])
            if not torch.allclose(policy, expected_policy, atol=1e-4, rtol=0) \
                    or abs(float(critic.squeeze()) - float(trace["critic_values"][step])) > 1e-4:
                raise RuntimeError("checkpoint does not reproduce this trajectory decision")
            summary, recorder = trace_model(
                model, inputs, digest, observation[4][0], sts2_sim.sts2_sim.__file__, temperature,
            )
            actions = env.action_descriptors()
            label_tensors(recorder, *decision_labels(env.concept_labels(), inputs, actions))
            summary["feature_schema"] = feature_schema(model)
            summary["decision"] = {
                "run": name, "trajectory": trace_id, "step": step,
                "actions": actions, "state": json.loads(env.state(0)),
            }
            removed = self.graphs.put(summary["graph_id"], (summary, recorder, model))
            self.graph_keys[cache_key] = summary["graph_id"]
            if removed:
                self.graph_keys = {key: value for key, value in self.graph_keys.items()
                                   if value not in removed}
            return summary

    def add(self, name, trace):
        with self.lock:
            run = self._run(name)
            trace = dict(validate_trace(trace))
            trace["id"] = f"imported:{uuid.uuid4().hex}"
            trace["source"] = "imported"
            self._remember(name, trace)
            return replay_trace(trace, _training_config(json.loads(
                (run / "run.json").read_text()
            ), trace))

    def generate(self, name, options):
        with self.lock:
            run = self._run(name)
            checkpoint_name = str(options.get("checkpoint", "latest.pt"))
            checkpoint_path = (run / checkpoint_name).resolve()
            if not checkpoint_path.is_relative_to(run) or not checkpoint_path.is_file():
                raise ValueError("invalid checkpoint")
            if options.get("snapshot", True):
                (run / "snapshots").mkdir(exist_ok=True)
                temporary = run / "snapshots" / f".{uuid.uuid4().hex}.tmp"
                shutil.copyfile(checkpoint_path, temporary)
                checkpoint_sha256 = sha256_file(temporary)
                checkpoint_path = run / "snapshots" / f"{checkpoint_sha256}.pt"
                os.replace(temporary, checkpoint_path)
            else:
                checkpoint_sha256 = sha256_file(checkpoint_path)
            model, checkpoint = load(checkpoint_path, torch.device("cpu"))
            model.eval()
            self.model = name, checkpoint_name, checkpoint_sha256, model
            manifest = json.loads((run / "run.json").read_text())
            sessions = manifest.get("sessions", [])
            training = sessions[-1].get("training", manifest.get("training", {})) \
                if sessions else manifest.get("training", {})
            stage = int(options.get("stage", checkpoint["stage"]))
            if not 0 <= stage < len(manifest.get("stages", STAGES)):
                raise ValueError("invalid stage")
            stage_values = manifest.get("stages", STAGES)[stage]
            if isinstance(stage_values, dict):
                default_ascension, default_bonus = stage_values["ascension"], stage_values["bonus"]
            else:
                default_ascension, default_bonus = stage_values
            ascension = int(options.get("ascension", default_ascension))
            bonus = int(options.get("bonus", default_bonus))
            temperature = float(options.get(
                "temperature", training.get("policy_temperature", .8)
            ))
            seed = int(options.get("seed", training.get("training_seed", 1)))
            runs = int(options.get("runs", 1))
            characters = [int(value) for value in options.get("characters", range(5))]
            max_steps = int(options.get("max_steps", training.get("max_steps", 2048)))
            max_combat_steps = int(options.get(
                "max_combat_steps", training.get("max_combat_steps", 512)
            ))
            sampled = bool(options.get("sample", True))
            if (not characters or any(not 0 <= value < 5 for value in characters)
                    or not 1 <= runs <= 64 or min(max_steps, max_combat_steps) < 1
                    or not math.isfinite(temperature) or temperature <= 0):
                raise ValueError("invalid generation settings")
            fingerprint = int(checkpoint["fingerprint"])
            policy = export_value_model(None, model, fingerprint, 1, 0, True)
            revision = int(checkpoint.get("revisions", {}).get(
                "policy_revision", checkpoint.get("decisions", 0)
            ))
            generated = []
            for character in characters:
                for run_index in range(runs):
                    trajectory = self._generate_one(
                        policy, fingerprint, checkpoint_name, revision,
                        seed + character * 1_000_000 + run_index, character, stage,
                        ascension, bonus, temperature, sampled, max_steps, max_combat_steps,
                    )
                    trajectory["checkpoint_sha256"] = checkpoint_sha256
                    self._remember(name, trajectory)
                    generated.append(_metadata(trajectory))
            return {"generated": generated}

    def explain(self, name, trace_id, step, case=None):
        with self.lock:
            trace = self._generated(name).get(trace_id)
            if trace is None or trace.get("source") != "generated":
                raise ValueError("explanations are available only for freshly generated trajectories")
            step = int(step)
            if not 0 <= step < len(trace["choices"]):
                raise ValueError("invalid trajectory step")
            key = name, trace_id, step, case
            if key in self.explanations:
                return self.explanations[key]
            checkpoint = str(trace["checkpoint"])
            digest = str(trace["checkpoint_sha256"])
            if self.model is None or self.model[:3] != (name, checkpoint, digest):
                path = _checkpoint_file(self._run(name), trace)
                if not path.is_relative_to(self._run(name)) or not path.is_file() \
                        or sha256_file(path) != digest:
                    raise ValueError("the exact generating checkpoint is no longer available")
                model, _ = load(path, torch.device("cpu")); model.eval()
                self.model = name, checkpoint, digest, model
            model = self.model[3]
            env = sts2_sim.Batch(
                1, int(trace["seed"]), int(trace["character"]),
                ascension=int(trace["ascension"]),
            )
            env.set_training_bonus(int(trace["bonus"]))
            for choice in trace["choices"][:step]:
                env.observe_tokens()
                env.step([int(choice)])
            observation = env.observe_tokens(flat=True)
            result = explain_decision(
                model, observation, json.loads(env.state(0)), env.action_descriptors(),
                trace, step, case,
            ) | {
                "run": name, "trajectory": trace_id,
                "checkpoint": checkpoint, "checkpoint_sha256": digest,
            }
            self.explanations[key] = result
            return result

    @staticmethod
    def _generate_one(policy, fingerprint, checkpoint, revision, seed, character, stage,
                      ascension, bonus, temperature, sampled, max_steps, max_combat_steps):
        env = sts2_sim.Batch(1, seed, character, ascension=ascension)
        env.set_training_bonus(bonus)
        env.load_policy(policy)
        trace = {
            "trajectory_schema": 2, "id": f"generated:{uuid.uuid4().hex}",
            "source": "generated", "time": time.time(), "checkpoint": checkpoint,
            "seed": int(env.seeds()[0]), "character": character, "stage": stage,
            "ascension": ascension, "bonus": bonus, "temperature": temperature,
            "sampled": sampled, "fingerprint": fingerprint, "choices": [],
            "log_policies": [], "critic_values": [], "canonical_progress": [],
            "phases": [], "policy_revisions": [],
        }
        started = time.monotonic()
        combat_steps = 0
        outcome = "step_cap"
        stats = env.stats()[0]
        for step in range(max_steps):
            if not env.has_legal_actions()[0]:
                outcome = "empty_actions"
                break
            _, policy_details, critic_value = env.policy_details([0], temperature)[0]
            result = env.policy(temperature, sampled, True, skip_forced=False)
            choice = int(result[1][0])
            trace["choices"].append(choice)
            trace["log_policies"].append([float(value) for value in policy_details])
            trace["critic_values"].append(float(critic_value))
            trace["canonical_progress"].append(int(result[12][0]))
            trace["phases"].append(int(result[13][0]))
            trace["policy_revisions"].append(revision)
            done, stats, legal, in_combat = bool(result[8][0]), result[9][0], \
                bool(result[10][0]), bool(result[11][0])
            combat_steps = combat_steps + 1 if in_combat else 0
            still_combat = stats[4] == 1
            if done:
                outcome = "won" if stats[4] == 12 else "dead"
                break
            if not legal:
                outcome = "empty_actions"
                break
            if combat_steps >= max_combat_steps and still_combat:
                outcome = "combat_cap"
                break
        trace["completion_seconds"] = time.monotonic() - started
        trace["terminal_floor"] = int((stats[0] - 1) * 17 + stats[1])
        trace["terminal"] = outcome in ("won", "dead")
        trace["outcome"] = outcome
        return validate_trace(trace)


def model_page():
    return r'''<!doctype html><meta charset=utf-8><title>Spirefysh model graph</title><style>
:root{color-scheme:dark;--bg:#0b0f15;--panel:#141a23;--line:#2c3848;--muted:#8f9daf;--text:#e8edf5;--blue:#78b7ff;--green:#68d69b;--amber:#e8c66a;--purple:#bd9cff}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:13px/1.4 Inter,system-ui,sans-serif}button,input{font:inherit;color:inherit;background:#202938;border:1px solid #46566d;border-radius:6px;padding:7px 9px}button{cursor:pointer}button:hover{border-color:var(--blue)}button.active{color:#07111e;background:var(--blue);border-color:var(--blue);font-weight:700}header{height:56px;padding:0 22px;display:flex;align-items:center;gap:18px;border-bottom:1px solid var(--line);background:#101620}header a{color:var(--blue);text-decoration:none}h1,h2,h3{margin:0}h1{font-size:18px}h2{font-size:17px}h3{font-size:13px}.spacer{flex:1}.muted{color:var(--muted)}.good{color:var(--green)}.error{color:#ef7777}main{max-width:1700px;margin:auto;padding:18px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:10px;overflow:hidden}.analysis-head{display:flex;align-items:center;gap:10px;padding:13px 15px;border-bottom:1px solid var(--line)}.toolbar{display:flex;gap:8px;padding:11px 12px;border-bottom:1px solid var(--line)}.toolbar input{flex:1}.segmented{display:flex}.segmented button{border-radius:0;margin-left:-1px}.segmented button:first-child{border-radius:6px 0 0 6px}.segmented button:last-child{border-radius:0 6px 6px 0}
.workspace{display:grid;grid-template-columns:minmax(760px,1.4fr) minmax(430px,.6fr);min-height:760px}.graph-pane{border-right:1px solid var(--line);min-width:0}.graph-header{display:flex;align-items:center;gap:7px;padding:10px 12px;border-bottom:1px solid var(--line)}.graph-header>button{padding:3px 8px;min-width:30px}.breadcrumb{display:flex;align-items:center;gap:7px}.canvas-wrap{overflow:auto;height:calc(100vh - 250px);min-height:600px;background-image:radial-gradient(#273243 1px,transparent 1px);background-size:20px 20px}#canvasSizer{position:relative}.canvas{position:relative;margin:10px;transform-origin:0 0;transition:transform .12s ease}.edges{position:absolute;inset:0;pointer-events:none;z-index:1}.edges path{fill:none;stroke:#46566b;stroke-width:1.5}.edges path.hot{stroke:#5688b6;stroke-width:2}.edges path.faded{opacity:.16}.edges marker path{fill:#46566b}
.node{position:absolute;z-index:2;width:148px;min-height:56px;padding:8px 9px;text-align:left;background:#18212d;border:1px solid #405066;border-radius:8px;box-shadow:0 4px 12px #0004}.node:hover{border-color:var(--blue);background:#1d2a39}.node.selected{border:2px solid var(--blue);background:#173049;padding:7px 8px}.node strong,.node code,.node small{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.node strong{font-size:12px}.node code{font:10px ui-monospace,SFMono-Regular,Menlo,monospace;color:#b8c9dc;margin-top:2px}.node small{font-size:10px;color:var(--muted);margin-top:4px}.node.input{border-top:3px solid #5e7b9f}.node.local{border-top:3px solid var(--purple)}.node.global{border-top:3px solid var(--blue)}.node.head{border-top:3px solid var(--green)}.node.leaf{border-style:dashed}.node.empty{opacity:.5}.node.dim{opacity:.14}.node.unrelated{opacity:.36}.node.sum-node{width:34px;min-height:34px;height:34px;padding:0;border-radius:50%;display:flex;align-items:center;justify-content:center}.node.sum-node strong{font-size:22px;line-height:1;font-weight:500}.node.sum-node.selected{padding:0}
.expanded-region{position:absolute;z-index:0;border:2px dashed var(--blue);border-radius:14px;background:#101720cc}.expanded-region.repeat{border-color:var(--amber)}.expanded-title{position:absolute;z-index:3;display:flex;align-items:center;gap:9px;padding:5px 8px;background:var(--panel);border-radius:6px;max-width:calc(100% - 20px)}.expanded-title code{color:var(--muted);font-size:11px}.expanded-title .fold{width:20px;min-width:20px;padding:0;border:0;background:transparent;color:var(--blue);font-size:17px;line-height:20px}
.inspector{padding:18px 20px;min-width:0;overflow:auto;max-height:calc(100vh - 160px)}.crumb{color:var(--muted);font-size:11px;margin-bottom:5px}.title h2{font-size:20px}.module{font:13px ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--blue);margin:4px 0 12px;overflow-wrap:anywhere}.pills{display:flex;flex-wrap:wrap;gap:6px;margin-bottom:15px}.pill{display:inline-block;padding:2px 7px;border:1px solid var(--line);border-radius:999px;color:var(--muted);font-size:10px}.stats,.rows{width:100%;border-collapse:collapse}.stats th,.stats td,.rows th,.rows td{padding:6px 8px;border-bottom:1px solid var(--line);text-align:right;font-variant-numeric:tabular-nums}.stats th:first-child,.stats td:first-child,.rows th:first-child,.rows td:first-child{text-align:left}.stats td:first-child{color:var(--muted)}.subhead{display:flex;gap:8px;align-items:center;margin:16px 0 7px}.formula{padding:11px;margin:12px 0;background:#10161f;border:1px solid var(--line);border-radius:7px;font:12px ui-monospace,SFMono-Regular,Menlo,monospace;color:#cbd8e8}.chooser{display:flex;flex-wrap:wrap;gap:5px;margin:14px 0 8px}.chooser button{padding:4px 8px;font-size:11px}.heatmap-scroll{overflow:auto;max-height:260px;padding:8px;background:#0d131c;border:1px solid var(--line);border-radius:7px}.heatmap-scroll canvas{display:block;image-rendering:pixelated}.heatmap-legend{display:grid;grid-template-columns:auto 1fr auto;gap:8px;align-items:center;margin-top:6px;color:var(--muted);font-size:10px}.heatmap-gradient{height:8px;background:linear-gradient(90deg,#3b4cc0,#8db0fe,#dddcdc,#f4987a,#b40426);border-radius:5px}.readout{min-height:18px;margin-top:5px;color:#cbd8e8;font:11px ui-monospace,SFMono-Regular,Menlo,monospace}.table-scroll{overflow:auto;max-height:420px}.op{font:11px ui-monospace,monospace;padding:4px 0;border-bottom:1px solid var(--line)}details{margin-top:16px}summary{cursor:pointer;color:var(--muted)}
.execution{display:none;padding:7px}.execution.visible{display:block}.execution button{width:100%;display:grid;grid-template-columns:44px 1fr auto;gap:8px;text-align:left;background:transparent;border:0;border-bottom:1px solid var(--line);border-radius:0}.execution code{display:block;color:var(--muted);font-size:10px}@media(max-width:1000px){.workspace{grid-template-columns:1fr}.graph-pane{border-right:0;border-bottom:1px solid var(--line)}}
</style><header><a href=/trajectories.html>← Trajectories</a><h1>Model graph</h1><span id=context class=muted></span><span class=spacer></span><span id=proof class=muted>loading</span></header><main><section class=panel><div class=analysis-head><h2>Decision analysis</h2><span id=status class=muted></span><span class=spacer></span><button id=download>Download JSON</button></div><div class=toolbar><input id=search placeholder="Find a module, operation, or tensor… (Enter to reveal)"><div class=segmented><button id=modelButton class=active>Model DAG</button><button id=rawButton>Raw operations</button><button id=executionButton>Execution</button></div><button id=foldAll>Fold all</button></div><div class=workspace><section class=graph-pane><div class=graph-header><div id=breadcrumb class=breadcrumb></div><span class=spacer></span><button id=zoomOut title="Zoom out">−</button><button id=zoomReset title="Reset zoom">100%</button><button id=zoomIn title="Zoom in">+</button><button id=zoomFit title="Fit graph">Fit</button></div><div id=canvasWrap class=canvas-wrap><div id=canvasSizer><div id=canvas class=canvas></div></div><div id=execution class=execution></div></div></section><aside id=inspector class=inspector></aside></div></section></main><script>
const $=id=>document.getElementById(id),params=new URLSearchParams(location.search),opened=new Set,CIRCLES={Add:'M10 4v12M4 10h12',Subtract:'M4 10h12',Multiply:'M5 5l10 10M15 5L5 15'};let payload,graph,byId={},children={},atoms={},inline=new Set,mode='model',selected,zoom=1,natural=[900,600],drawnEdges=[];
function esc(value){return String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}async function post(body){const response=await fetch('/api/model-graph',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}),text=await response.text();if(!response.ok)throw Error(text.match(/<p>Message: ([^<]+)/)?.[1]||text);return JSON.parse(text)}
function unique(pairs){return[...new Map(pairs.map(pair=>[pair.join('>'),pair])).values()]}function ancestors(id){const result=[];for(let key=byId[id]?.parent;key;key=byId[key].parent)result.push(key);return result}function shape(node){return node.shape?node.shape.join(' × '):''}
function automaticLayout(ids,edges,{sizes={},columnGap=48,rowGap=18,padding=25,optimize=true}={}){const members=new Set(ids),relevant=edges.filter(([from,to])=>members.has(from)&&members.has(to)),rank=Object.fromEntries(ids.map(id=>[id,0]));for(let pass=0;pass<ids.length;pass++){let changed=false;for(const [from,to] of relevant)if(rank[to]<rank[from]+1){rank[to]=rank[from]+1;changed=true}if(!changed)break}const columns=[];for(const id of ids)(columns[rank[id]]??=[]).push(id);for(let index=0;index<columns.length;index++)columns[index]??=[];const preferred=columns.map(column=>[...column]),initial=Object.fromEntries(ids.map((id,index)=>[id,index])),preferredPosition=Object.fromEntries(preferred.flatMap(column=>column.map((id,index)=>[id,index])));
const crossings=(layout=columns)=>{const order={};layout.forEach(column=>column.forEach((id,index)=>order[id]=index));let count=0;for(let left=0;left<relevant.length;left++)for(let right=left+1;right<relevant.length;right++){const [a,b]=relevant[left],[c,d]=relevant[right];if(rank[a]===rank[c]&&rank[b]===rank[d]&&a!==c&&b!==d&&(order[a]-order[c])*(order[b]-order[d])<0)count++}return count},displacement=layout=>layout.reduce((sum,column)=>sum+column.reduce((value,id,index)=>value+Math.abs(index-preferredPosition[id]),0),0);
if(optimize){const sweep=forward=>{const sequence=forward?[...columns.keys()].slice(1):[...columns.keys()].slice(0,-1).reverse();for(const column of sequence){const order=new Map(columns[forward?column-1:column+1].map((id,index)=>[id,index])),neighbors=id=>relevant.filter(edge=>edge[forward?1:0]===id).map(edge=>order.get(edge[forward?0:1])).filter(value=>value!==undefined),score=id=>{const values=neighbors(id);return values.length?values.reduce((sum,value)=>sum+value,0)/values.length:initial[id]};columns[column].sort((left,right)=>score(left)-score(right)||initial[left]-initial[right])}};for(let pass=0;pass<3;pass++){sweep(true);sweep(false)}
const better=(left,right)=>left[0]<right[0]||left[0]===right[0]&&left[1]<right[1],improve=(layout,passes)=>{for(let pass=0;pass<passes;pass++){let changed=false;for(const column of layout){let score=[crossings(layout),displacement(layout)],swap=null;for(let left=0;left<column.length;left++)for(let right=left+1;right<column.length;right++){[column[left],column[right]]=[column[right],column[left]];const current=[crossings(layout),displacement(layout)];[column[left],column[right]]=[column[right],column[left]];if(better(current,score)){score=current;swap=[left,right]}}if(swap){[column[swap[0]],column[swap[1]]]=[column[swap[1]],column[swap[0]]];changed=true}}if(!changed)break}};if(relevant.length<400){improve(columns,4);let random=2166136261,bestLayout=columns.map(column=>[...column]),bestScore=[crossings(),displacement(columns)];const next=()=>((random=Math.imul(random^random>>>13,16777619)>>>0)/2**32);for(let restart=0;restart<(relevant.length<80?48:6);restart++){const candidate=preferred.map(column=>[...column]);for(const column of candidate)for(let index=column.length-1;index>0;index--){const target=Math.floor(next()*(index+1));[column[index],column[target]]=[column[target],column[index]]}improve(candidate,6);const score=[crossings(candidate),displacement(candidate)];if(better(score,bestScore)){bestScore=score;bestLayout=candidate.map(column=>[...column])}}columns.splice(0,columns.length,...bestLayout)}}
const size=id=>sizes[id]||{width:148,height:56},widths=columns.map(column=>Math.max(0,...column.map(id=>size(id).width))),heights=columns.map(column=>column.reduce((sum,id)=>sum+size(id).height,0)+Math.max(0,column.length-1)*rowGap),initialHeight=Math.max(0,...heights),positions={};let x=padding;columns.forEach((column,index)=>{let y=padding+(initialHeight-heights[index])/2;for(const id of column){positions[id]=[x,y];y+=size(id).height+rowGap}x+=widths[index]+columnGap});
for(let index=1;index<columns.length;index++){const column=columns[index];if(!column.length)continue;const desired=column.map(id=>{const parents=relevant.filter(([,to])=>to===id).map(([from])=>positions[from][1]+size(from).height/2);return parents.length?parents.reduce((sum,value)=>sum+value,0)/parents.length-size(id).height/2:positions[id][1]}),placed=[];let cursor=padding;for(let row=0;row<column.length;row++){placed[row]=Math.max(desired[row],cursor);cursor=placed[row]+size(column[row]).height+rowGap}const adjustment=desired.reduce((sum,value,row)=>sum+value-placed[row],0)/column.length,min=Math.min(...placed)+adjustment,shift=min<padding?padding-min:0;column.forEach((id,row)=>positions[id][1]=placed[row]+adjustment+shift)}
return{positions,width:x-columnGap+padding,height:Math.max(0,...ids.map(id=>positions[id][1]+size(id).height))+padding,crossings:crossings()}}
function prepare(){graph=payload.graph;byId=Object.fromEntries(graph.nodes.map(node=>[node.id,node]));children={};for(const node of graph.nodes)if(node.parent)(children[node.parent]??=[]).push(node.id);atoms={};const collect=id=>atoms[id]??=children[id]?children[id].flatMap(collect):[id];collect('root');inline=new Set(graph.inline)}
function items(id){return(children[id]||[]).flatMap(child=>inline.has(child)&&!opened.has(child)?items(child):byId[child].leaf&&!opened.has(id)?[]:[child])}function circle(id){const node=byId[id];return CIRCLES[node.kind]&&node.label===node.kind&&!children[id]}function size(id){return circle(id)?{width:34,height:34}:{width:148,height:56}}
function layoutScope(id){const ids=items(id),sizes={},inner={},owner=new Map;for(const item of ids){if(opened.has(item)){inner[item]=layoutScope(item);sizes[item]={width:Math.max(inner[item].width,260)+24,height:inner[item].height+52}}else sizes[item]=size(item);for(const atom of atoms[item])owner.set(atom,item)}const edges=unique(graph.edges.map(edge=>[owner.get(edge.from),owner.get(edge.to)]).filter(([from,to])=>from&&to&&from!==to)),view=automaticLayout(ids,edges,{sizes,columnGap:id==='root'?55:34,padding:id==='root'?25:14});return{...view,ids,sizes,inner,crossings:view.crossings+Object.values(inner).reduce((sum,value)=>sum+value.crossings,0)}}
function place(view,x,y,out){for(const id of view.ids){const [left,top]=view.positions[id],X=x+left,Y=y+top;if(view.inner[id]){out.regions.push({id,x:X,y:Y,...view.sizes[id]});place(view.inner[id],X+12,Y+40,out)}else out.nodes.push({id,x:X,y:Y,...view.sizes[id]})}return out}
function edgePath([from,to],position){const A=position[from],B=position[to],x1=A.x+A.width,y1=A.y+A.height/2,x2=B.x,y2=B.y+B.height/2,middle=(x1+x2)/2;return`<path data-from="${esc(from)}" data-to="${esc(to)}" d="M${x1} ${y1}C${middle} ${y1},${middle} ${y2},${x2} ${y2}" marker-end="url(#arrow)"/>`}
function nodeMarkup({id,x,y}){const node=byId[id],group=children[id],classes=`node ${node.role}${id===selected?' selected':''}${node.shape?.[0]===0?' empty':''}${node.leaf?' leaf':''}`;return circle(id)?`<button class="${classes} sum-node" data-id="${esc(id)}" style="left:${x}px;top:${y}px" title="${esc(node.label)} · ${esc(shape(node))}"><svg viewBox="0 0 20 20" width=18 height=18><path d="${CIRCLES[node.kind]}" stroke=currentColor stroke-width=2 stroke-linecap=round /></svg></button>`:`<button class="${classes}" data-id="${esc(id)}" style="left:${x}px;top:${y}px"><strong>${esc(node.label)}</strong><code>${esc(node.code)}</code><small>${esc(shape(node))}${group?` · ${node.size} op${node.size===1?'':'s'} · expand ⤢`:` · ${esc(node.kind)}`}</small></button>`}
function regionMarkup({id,x,y,width,height}){const node=byId[id];return`<div class="expanded-region ${node.kind==='repeat'?'repeat':''}" style="left:${x}px;top:${y}px;width:${width}px;height:${height}px"></div><div class=expanded-title style="left:${x+10}px;top:${y+7}px"><button class=fold data-fold="${esc(id)}" title=Fold>▾</button><b>${esc(node.label)}</b><code>${esc(node.code)}</code></div>`}
function setCanvasSize(width,height){natural=[width,height];$('canvas').style.width=width+'px';$('canvas').style.height=height+'px';setZoom(zoom)}function setZoom(value){zoom=Math.max(.2,Math.min(1.6,value));$('canvas').style.transform=`scale(${zoom})`;$('canvasSizer').style.width=natural[0]*zoom+20+'px';$('canvasSizer').style.height=natural[1]*zoom+20+'px';$('zoomReset').textContent=Math.round(zoom*100)+'%'}
function draw(out,width,height,summary){const position=Object.fromEntries(out.nodes.map(node=>[node.id,node])),owner=new Map;for(const node of out.nodes)for(const atom of atoms[node.id]||[node.id])owner.set(atom,node.id);drawnEdges=unique(graph.edges.map(edge=>[owner.get(edge.from),owner.get(edge.to)]).filter(([from,to])=>from&&to&&from!==to));setCanvasSize(width,height);$('canvas').innerHTML=`<svg class=edges width=${width} height=${height}><defs><marker id=arrow viewBox="0 0 10 10" refX=9 refY=5 markerWidth=5 markerHeight=5 orient=auto-start-reverse><path d="M0 0L10 5L0 10z"/></marker></defs>${drawnEdges.map(edge=>edgePath(edge,position)).join('')}</svg>`+out.regions.map(regionMarkup).join('')+out.nodes.map(nodeMarkup).join('');$('breadcrumb').innerHTML=`<b>Whole model</b><span class=muted>${summary}</span>`;bind()}
function render(){if(mode==='raw')return renderRaw();const view=layoutScope('root');draw(place(view,0,0,{nodes:[],regions:[]}),view.width+20,view.height+20,`${opened.size} subgraph${opened.size===1?'':'s'} open · ${view.crossings} layered crossings`)}
function renderRaw(){const producer={},edges=[];for(const op of graph.raw_ops)for(const tensor of op.outputs)producer[tensor]=op.id;for(const op of graph.raw_ops)for(const tensor of op.inputs)if(producer[tensor])edges.push([producer[tensor],op.id]);for(const op of graph.raw_ops)byId[op.id]??={id:op.id,label:op.op.replace('aten.',''),code:op.source?`${op.source.function}:${op.source.line}`:'runtime',kind:'raw',role:'local',raw:true};const ids=graph.raw_ops.map(op=>op.id),view=automaticLayout(ids,unique(edges),{optimize:false});draw({nodes:ids.map(id=>({id,x:view.positions[id][0],y:view.positions[id][1],width:148,height:56})),regions:[]},view.width+20,view.height+20,`${ids.length} raw operations`)}
function highlight(id){if(!document.querySelector(`#canvas [data-id="${CSS.escape(id??'')}"]`)){document.querySelectorAll('#canvas .unrelated,#canvas .hot,#canvas .faded').forEach(element=>element.classList.remove('unrelated','hot','faded'));return}const up=new Set([id]),down=new Set([id]);for(let changed=true;changed;){changed=false;for(const [from,to] of drawnEdges)if(up.has(to)&&!up.has(from)){up.add(from);changed=true}}for(let changed=true;changed;){changed=false;for(const [from,to] of drawnEdges)if(down.has(from)&&!down.has(to)){down.add(to);changed=true}}document.querySelectorAll('#canvas .node').forEach(node=>node.classList.toggle('unrelated',!up.has(node.dataset.id)&&!down.has(node.dataset.id)));document.querySelectorAll('#canvas path[data-from]').forEach(path=>{const hot=up.has(path.dataset.from)&&up.has(path.dataset.to)||down.has(path.dataset.from)&&down.has(path.dataset.to);path.classList.toggle('hot',hot);path.classList.toggle('faded',!hot)})}
function bind(){document.querySelectorAll('#canvas .node').forEach(element=>{element.onclick=()=>{const id=element.dataset.id;if(children[id]){opened.add(id);render()}select(id)};element.onmouseenter=()=>highlight(element.dataset.id);element.onmouseleave=()=>highlight(selected)});document.querySelectorAll('[data-fold]').forEach(button=>button.onclick=()=>{const id=button.dataset.fold;for(const key of [...opened])if(key===id||ancestors(key).includes(id))opened.delete(key);render();select(id)});applySearch();highlight(selected)}
function select(id){selected=id;document.querySelectorAll('#canvas .node').forEach(node=>node.classList.toggle('selected',node.dataset.id===id));highlight(id);inspect(id)}function reveal(id){mode='model';for(const key of ancestors(id))if(key!=='root'&&!inline.has(key))opened.add(key);activate();select(id);document.querySelector(`#canvas [data-id="${CSS.escape(id)}"]`)?.scrollIntoView({block:'center',inline:'center'})}
function heatColor(value){const stops=[[59,76,192],[141,176,254],[221,220,220],[244,152,122],[180,4,38]],scaled=Math.max(0,Math.min(1,(value+3)/6))*(stops.length-1),left=Math.min(stops.length-2,Math.floor(scaled)),amount=scaled-left;return`rgb(${stops[left].map((channel,index)=>Math.round(channel+(stops[left+1][index]-channel)*amount)).join(',')})`}
function decode(tensor){if(!tensor?.data)return null;const bytes=Uint8Array.from(atob(tensor.data),c=>c.charCodeAt(0));return new Float32Array(bytes.buffer)}function grid(tensor){const values=decode(tensor);if(!values||!values.length)return null;const columns=tensor.shape.length?tensor.shape.at(-1):1;return{values,columns,rows:values.length/columns,labels:tensor.row_labels||[],channels:tensor.channel_labels||[]}}
function drawHeatmap(tensor){const data=grid(tensor),canvas=$('heatmap');if(!data||!canvas)return;const cellX=data.columns>512?1:data.columns>256?2:data.columns>64?4:12,cellY=data.rows===1?20:data.rows>150?2:data.rows>40?5:12,context=canvas.getContext('2d');canvas.width=data.columns*cellX;canvas.height=data.rows*cellY;for(let row=0;row<data.rows;row++)for(let column=0;column<data.columns;column++){context.fillStyle=heatColor(data.values[row*data.columns+column]);context.fillRect(column*cellX,row*cellY,cellX,cellY)}canvas.onmousemove=event=>{const bounds=canvas.getBoundingClientRect(),column=Math.min(data.columns-1,Math.floor((event.clientX-bounds.left)/cellX)),row=Math.min(data.rows-1,Math.floor((event.clientY-bounds.top)/cellY)),label=data.labels[row];const name=data.channels[column],value=data.values[row*data.columns+column];$('readout').textContent=`row ${row}${label?` (${label})`:''} · channel ${data.indices?.[column]??column}${name?` (${channelName(name)})`:''} · ${name?amount(name,value):value.toFixed(6)}`}}
function rowsTable(tensor){const data=grid(tensor);if(!data||data.rows<2)return'';const rows=[...Array(data.rows).keys()].map(row=>{const values=data.values.subarray(row*data.columns,(row+1)*data.columns);let squares=0,min=Infinity,max=-Infinity;for(const value of values){squares+=value*value;min=Math.min(min,value);max=Math.max(max,value)}return{row,rms:Math.sqrt(squares/values.length),min,max}}).sort((left,right)=>right.rms-left.rms).slice(0,10);return`<div class=subhead><h3>Rows</h3><span class=muted>sorted by RMS · top ${rows.length} of ${data.rows}</span></div><table class=rows><thead><tr><th>Row</th><th>RMS</th><th>Minimum</th><th>Maximum</th></tr></thead><tbody>${rows.map(row=>`<tr><td>${row.row} ${esc(data.labels[row.row]||'')}</td><td>${row.rms.toFixed(4)}</td><td>${row.min.toFixed(4)}</td><td>${row.max.toFixed(4)}</td></tr>`).join('')}</tbody></table>`}
function channelsTable(tensor,latent){const data=grid(tensor);if(!data)return'';if(!data.channels.length)return latent?'<p class=muted>Channels are learned latent features with no fixed meaning.</p>':'';const channels=[...Array(data.columns).keys()].map(channel=>{let squares=0;for(let row=0;row<data.rows;row++)squares+=data.values[row*data.columns+channel]**2;return{channel,rms:Math.sqrt(squares/data.rows)}}).sort((left,right)=>right.rms-left.rms).slice(0,12);return`<div class=subhead><h3>Largest channels</h3><span class=muted>by RMS over rows</span></div><table class=rows><thead><tr><th>Channel</th><th>RMS</th></tr></thead><tbody>${channels.map(row=>`<tr><td>${row.channel} ${esc(data.channels[row.channel]||'')}</td><td>${row.rms.toFixed(4)}</td></tr>`).join('')}</tbody></table>`}
function decodedTable(tensor){return`<div class=subhead><h3>Values</h3><span class=muted>IDs decoded with the table they index</span></div><table class=rows><thead><tr><th>Row</th><th>Active IDs</th></tr></thead><tbody>${tensor.decoded.map((names,row)=>`<tr><td>${row} ${esc(tensor.row_labels?.[row]||'')}</td><td style="text-align:left">${esc(names.join(', ')||'—')}</td></tr>`).join('')}</tbody></table>`}
function channelName(name){return name.replace(/ ÷ \d+$/,'')}function amount(name,value){const divisor=name.match(/ ÷ (\d+)$/)?.[1];return divisor?`${+(value*divisor).toFixed(3)} (÷${divisor} = ${value.toFixed(3)})`:value.toFixed(3)}
function channelTable(tensor){const data=grid(tensor),shown=Math.min(data.rows,12),rows=[...Array(shown).keys()];return`<div class=subhead><h3>Channels</h3><span class=muted>${data.columns} channels · one row per channel${data.rows>shown?` · first ${shown} of ${data.rows} rows`:''}</span></div><div class=table-scroll><table class=rows><thead><tr><th>Channel</th>${rows.map(row=>`<th>${esc(data.labels[row]||row)}</th>`).join('')}</tr></thead><tbody>${data.channels.map((name,column)=>`<tr><td>${esc(channelName(name||`channel ${column}`))}</td>${rows.map(row=>{const value=data.values[row*data.columns+column];return`<td>${value?esc(amount(name||'',value)):'<span class=muted>0</span>'}</td>`}).join('')}</tr>`).join('')}</tbody></table></div>`}
function showTensor(tensors,key){const tensor=tensors[key];document.querySelectorAll('.chooser button').forEach(button=>button.classList.toggle('active',button.dataset.key===key));$('tensorView').innerHTML=tensor.decoded?decodedTable(tensor):tensor.data?`<div class=subhead><h3>Activation heatmap</h3><span class=muted>coolwarm · fixed scale</span></div><div class=heatmap-scroll><canvas id=heatmap></canvas></div><div class=heatmap-legend><span>−3</span><div class=heatmap-gradient></div><span>+3</span></div><div id=readout class=readout>Move over a cell to inspect its value.</div>${graph.tensors[key]?.origin==='input'&&tensor.channel_labels?channelTable(tensor):rowsTable(tensor)+channelsTable(tensor,graph.tensors[key]?.origin==='activation')}`:`<p class=muted>${tensor.shape.join(' × ')} · too large to transfer; statistics only.</p>`;if(!tensor.decoded)drawHeatmap(tensor)}
async function inspect(id){const node=byId[id];if(!node)return;const rewrite=graph.rewrites.find(row=>row.node===id),pills=[node.kind,!node.leaf&&`${node.size} semantic op${node.size===1?'':'s'}`,!node.leaf&&node.role,rewrite&&`verified rewrite · max error ${rewrite.maximum_error.toExponential(1)}`].filter(Boolean),head=`<div class=crumb>${esc(ancestors(id).reverse().map(key=>byId[key].label).join(' / '))}</div><div class=title><h2>${esc(node.label)}</h2></div><div class=module>${esc(node.code)}</div><div class=pills>${pills.map(pill=>`<span class=pill>${esc(pill)}</span>`).join('')}</div>${rewrite?`<div class=formula>${esc(rewrite.label)}</div>`:''}`;if(node.raw){$('inspector').innerHTML=head+`<pre>${esc(JSON.stringify(graph.raw_ops.find(op=>op.id===id),null,2))}</pre>`;return}$('inspector').innerHTML=head+'<p class=muted>Loading exact tensors…</p>';try{const detail=await post({graph_id:payload.graph_id,node_id:id});if(selected!==id)return;const columns=[...node.outputs.map((key,index)=>[key,node.leaf?'Value':node.outputs.length>1?`Output ${index+1}`:'Output']),...node.inputs.map((key,index)=>[key,node.inputs.length>1?`Input ${index+1}`:'Input'])].filter(([key])=>detail.tensors[key]),tensors=Object.fromEntries([...columns.map(([key,label])=>[key,{...detail.tensors[key],label}]),...Object.entries(detail.parameters).map(([name,tensor])=>['parameter:'+name,{...tensor,label:name}])]),shown=columns.slice(0,4),stat=(label,key,digits=6)=>`<tr><td>${label}</td>${shown.map(([tensor])=>`<td>${detail.tensors[tensor][key]?.toFixed?.(digits)??'—'}</td>`).join('')}</tr>`,operations=atoms[id].filter(key=>byId[key].op_ids).flatMap(key=>byId[key].op_ids);
$('inspector').innerHTML=head+(shown.length?`<table class=stats><thead><tr><th>Statistic</th>${shown.map(([,label])=>`<th>${label}</th>`).join('')}</tr></thead><tbody><tr><td>Shape</td>${shown.map(([tensor])=>`<td>${detail.tensors[tensor].shape.join(' × ')}</td>`).join('')}</tr>${stat('Mean','mean')}${stat('Standard deviation','stddev')}${stat('RMS','rms')}${stat('Minimum','minimum')}${stat('Maximum','maximum')}</tbody></table>`:'')+(Object.keys(tensors).length?`<div class=subhead><h3>Tensors</h3></div><div class=chooser>${Object.entries(tensors).map(([key,tensor])=>`<button data-key="${esc(key)}">${esc(tensor.label)} · ${tensor.shape.join('×')}</button>`).join('')}</div><div id=tensorView></div>`:'<p class=muted>No tensors cross this boundary.</p>')+(operations.length?`<details><summary>${operations.length} underlying operation${operations.length===1?'':'s'}</summary>${operations.slice(0,300).map(key=>{const op=graph.raw_ops[Number(key.slice(1))];return`<div class=op>${esc(op.op)}${op.source?` · ${esc(op.source.function)}:${op.source.line}`:''}</div>`}).join('')}</details>`:'');document.querySelectorAll('.chooser button').forEach(button=>button.onclick=()=>showTensor(tensors,button.dataset.key));if(Object.keys(tensors).length)showTensor(tensors,Object.keys(tensors)[0])}catch(error){$('inspector').innerHTML=head+`<p class=error>${esc(error.message)}</p>`}}
function applySearch(){const query=$('search').value.toLowerCase();document.querySelectorAll('#canvas .node').forEach(node=>node.classList.toggle('dim',!!query&&!`${byId[node.dataset.id]?.label} ${byId[node.dataset.id]?.code}`.toLowerCase().includes(query)))}
function activate(){$('canvasSizer').style.display=mode==='execution'?'none':'block';$('execution').classList.toggle('visible',mode==='execution');for(const [button,name] of [['modelButton','model'],['rawButton','raw'],['executionButton','execution']])$(button).classList.toggle('active',mode===name);if(mode==='execution')$('breadcrumb').innerHTML='<b>Execution order</b><span class=muted>every semantic operation</span>';else render()}
$('search').oninput=applySearch;$('search').onkeydown=event=>{const query=$('search').value.toLowerCase();if(event.key!=='Enter'||!query)return;const match=graph.nodes.find(node=>node.id!=='root'&&`${node.label} ${node.code}`.toLowerCase().includes(query));if(match)reveal(match.id)};$('modelButton').onclick=()=>{mode='model';activate()};$('rawButton').onclick=()=>{mode='raw';activate()};$('executionButton').onclick=()=>{mode='execution';activate()};$('foldAll').onclick=()=>{opened.clear();mode='model';activate()};
$('zoomOut').onclick=()=>setZoom(zoom-.1);$('zoomIn').onclick=()=>setZoom(zoom+.1);$('zoomReset').onclick=()=>setZoom(1);$('zoomFit').onclick=()=>setZoom(Math.min(($('canvasWrap').clientWidth-20)/natural[0],($('canvasWrap').clientHeight-20)/natural[1]));$('canvasWrap').addEventListener('wheel',event=>{if(!event.ctrlKey&&!event.metaKey||mode==='execution')return;event.preventDefault();const wrap=$('canvasWrap'),previous=zoom,rect=wrap.getBoundingClientRect(),x=(wrap.scrollLeft+event.clientX-rect.left)/previous,y=(wrap.scrollTop+event.clientY-rect.top)/previous;setZoom(zoom+(event.deltaY<0?.1:-.1));wrap.scrollLeft=x*zoom-event.clientX+rect.left;wrap.scrollTop=y*zoom-event.clientY+rect.top},{passive:false});
$('download').onclick=()=>{if(!payload)return;const link=document.createElement('a');link.href=URL.createObjectURL(new Blob([JSON.stringify(payload)],{type:'application/json'}));link.download=`model-graph-${payload.graph_id.slice(0,8)}.json`;link.click();URL.revokeObjectURL(link.href)};
async function load(){try{payload=await post({run:params.get('run'),trajectory:params.get('trajectory'),step:Number(params.get('step'))});prepare();const proof=payload.proof;$('context').textContent=`/ ${params.get('run')} / decision ${Number(params.get('step'))+1}`;$('proof').innerHTML=`<span class=good>● verified</span> · ${proof.raw_operations} operations → ${proof.semantic_nodes} semantic nodes · ${proof.verified_rewrites} verified rewrites · max error ${proof.maximum_output_error}`;$('status').textContent=`model ${proof.model_version} / features ${proof.feature_version} · ${proof.checkpoint_sha256.slice(0,8)}…`;$('execution').innerHTML=graph.execution.map((id,index)=>{const node=byId[id];return`<button data-reveal="${esc(id)}"><span class=muted>${String(index+1).padStart(4,'0')}</span><span>${esc(node.label)}<code>${esc(ancestors(id).reverse().slice(1).map(key=>byId[key].label).join(' / '))}</code></span><span class=muted>${esc(shape(node))}</span></button>`}).join('');document.querySelectorAll('[data-reveal]').forEach(button=>button.onclick=()=>reveal(button.dataset.reveal));$('inspector').innerHTML=`<div class=crumb>Proof</div><div class=title><h2>Select a node</h2></div><p class=muted>Every node is computed from the exact operations of this checkpoint and decision. Click a group to open it in place; fold it with ▾.</p>`;render()}catch(error){$('proof').innerHTML='<span class=error>● unverified</span>';$('status').textContent=error.message;$('canvas').innerHTML=`<p class=error>${esc(error.message)}</p>`}}load();
</script>'''


def trajectory_page():
    return r'''<!doctype html><meta charset=utf-8><title>Spirefysh trajectories</title><link rel=stylesheet href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css"><script src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script><style>
*{box-sizing:border-box}body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}a{color:#77b7ff}h1,h2,h3{margin:0 0 10px}.bar,.controls,.playback{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin:12px 0}button,select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}button{cursor:pointer}.grid{display:grid;grid-template-columns:minmax(280px,380px) 1fr;gap:14px}.panel{padding:14px;background:#171d28;border:1px solid #303a49;border-radius:10px;min-width:0}.list{max-height:70vh;overflow:auto}.trace{display:block;width:100%;text-align:left;margin:5px 0}.trace.active{border-color:#77b7ff}.muted{color:#9aa8bb}.phase,.inventory,.explanation,.training-signal{margin:18px 0}.cards,.signal-values{display:flex;gap:5px;flex-wrap:wrap}.chip{padding:4px 7px;background:#252d3a;border-radius:5px}.item,.keyword{position:relative;border-bottom:1px dotted #9aa8bb}.keyword,.accent,.actor-heading span{color:#efc76b}.relic-line{display:flex;gap:3px;overflow-x:auto;margin:8px 0 18px}.relic-icon,.effect-icon{position:relative;flex:0 0 auto;border:0}.relic-icon{display:grid;place-items:center;width:46px;height:46px;flex-basis:46px}.relic-icon>.preview-art{margin:0;zoom:.55}.actor-sprite>.preview-art{margin:0}.combatants{display:flex;justify-content:space-between;gap:40px;overflow-x:auto;padding:12px 0 20px}.friendlies,.enemies{display:grid;grid-auto-flow:column;grid-auto-columns:180px;gap:12px}.actor{width:180px;text-align:center}.actor-heading{min-height:42px}.actor-heading span{display:block}.actor-sprite{height:180px}.vitals{display:flex;gap:6px;align-items:center}.health{position:relative;flex:1;height:24px;overflow:hidden;border:2px solid #d6b798;border-radius:5px;background:#32191b}.health>span{display:block;height:100%;background:linear-gradient(#df5858,#8e2029)}.health>b{position:absolute;inset:1px;text-shadow:0 1px 2px #000}.block{display:flex;align-items:center;justify-content:center;min-width:42px;height:36px;background:url('/game-assets/block.webp') center/42px no-repeat;text-shadow:0 1px 3px #000}.effect-line{display:flex;justify-content:safe center;height:38px;gap:3px;margin-top:6px;overflow-x:auto}.effect-icon{display:grid;place-items:center;width:34px;height:34px;flex-basis:34px}.effect-icon>.preview-art{margin:0;zoom:.5}.missing-icon{display:flex;align-items:center;justify-content:center;background:#252d3a;border-radius:50%;color:#9aa8bb}.combat-resources{display:grid;grid-template-columns:2fr 1fr;gap:12px}.actions,.influences,.heatmap{width:100%;border-collapse:collapse}.actions th,.actions td,.influences th,.influences td,.heatmap th,.heatmap td{padding:7px;border-bottom:1px solid #303a49;text-align:left;vertical-align:top}.actions tr.selected{background:#173a2a}.action-name{position:relative;display:inline-block;border-bottom:1px dotted #9aa8bb}.preview{display:none;position:fixed;z-index:1000;left:0;top:0;width:280px;max-height:calc(100vh - 16px);overflow:auto;padding:12px;background:#0d1118;border:1px solid #526176;border-radius:7px;box-shadow:0 8px 24px #000;text-align:left;font-weight:400;color:#e8ecf2}.action-name:hover>.preview,.item:hover>.preview,.keyword:hover>.preview,.relic-icon:hover>.preview,.effect-icon:hover>.preview{display:block}.preview-art{display:block;margin:0 auto 10px;background-repeat:no-repeat}.preview b{font-size:16px}.preview-description{display:block;margin:7px 0;white-space:normal}.card-popup{width:276px;padding:12px}.full-card{display:block;position:relative;width:250px;height:380px;overflow:hidden;border:3px solid #8e7560;border-radius:16px;background:linear-gradient(#602820 0 62%,#261b18 62%);box-shadow:inset 0 0 25px #000}.full-card.skill{background:linear-gradient(#315275 0 62%,#172536 62%)}.full-card.power{background:linear-gradient(#684b78 0 62%,#2d2034 62%)}.full-card.status,.full-card.curse{background:linear-gradient(#5b5b5b 0 62%,#282828 62%)}.full-card .preview-art{position:absolute;top:43px;left:-3px;margin:0}.card-title{position:absolute;z-index:1;top:10px;left:32px;right:8px;text-align:center;text-shadow:0 2px 3px #000}.card-cost{position:absolute;z-index:2;top:7px;left:6px;width:31px;height:31px;padding-top:5px;border-radius:50%;background:#315cba;text-align:center;font-size:18px}.card-description{position:absolute;top:242px;left:12px;right:12px;bottom:27px;display:flex;align-items:center;justify-content:center;overflow:auto;text-align:center;font-size:12px;line-height:1.35}.card-type{position:absolute;bottom:7px;left:10px;right:10px;text-align:center;color:#c9bba9;font-size:11px}.attention{overflow:auto;max-height:600px}.heatmap td{min-width:38px;text-align:center}.signal-values .chip b{color:#77b7ff}.formula-line{display:flex;align-items:center;flex-wrap:wrap;gap:7px;min-height:42px;border-left:2px solid #303a49;padding-left:12px}.formula-term{border:0;border-bottom:1px dotted #77b7ff;border-radius:0;background:transparent;color:#e8ecf2;padding:2px}.formula-term[aria-expanded=true]{color:#77b7ff}.formula-leaf{display:inline-block;border-bottom:1px dotted #9aa8bb;padding:2px}.error{color:#ee7b7b}pre{white-space:pre-wrap;word-break:break-word}input[type=range]{flex:1;min-width:240px}@media(max-width:900px){.grid,.combat-resources{grid-template-columns:1fr}}
.preview{width:min(280px,calc(100vw - 16px))}
</style><div class=bar><a href=/>&larr; Metrics</a><h1>Trajectories</h1><span id=status class=muted></span></div>
<div class=panel><div class=controls><label>Run <select id=run></select></label><label>Session <select id=session><option value="">All</option></select></label><label>Character <select id=character><option value="">All</option></select></label><label>Outcome <select id=outcome><option value="">All</option><option>won</option><option>dead</option><option>step_cap</option><option>combat_cap</option><option>empty_actions</option></select></label><label>Minimum floor <input id=floor type=number min=0 value=0></label><label>Seed <input id=seedFilter type=number placeholder=any></label><label>Policy revision <input id=revision type=number placeholder=any></label><button id=refresh>Refresh</button><label>Import <input id=import type=file accept=application/json></label></div>
<details><summary>Generate from checkpoint</summary><div class=controls><label>Checkpoint <select id=checkpoint></select></label><label>Seed <input id=seed type=number></label><label>Runs / character <input id=runs type=number min=1 max=64 value=1></label><label>Mode <select id=mode><option value=sample>Sampled</option><option value=greedy>Greedy</option></select></label><label>Temperature <input id=temperature type=number min=.001 step=.05></label><label>Stage <input id=stage type=number min=0 placeholder=checkpoint></label><label>Ascension <input id=ascension type=number min=0 placeholder=stage></label><label>Bonus <input id=bonus type=number min=0 placeholder=stage></label><label>Max steps <input id=maxSteps type=number min=1></label><label>Max combat steps <input id=maxCombat type=number min=1></label><label title="Copy the checkpoint into snapshots/ so activation exploration survives it being overwritten"><input id=snapshot type=checkbox checked> Keep checkpoint copy</label><span id=characters></span><button id=generate>Generate</button><span id=generationNote class=error></span></div></details></div>
<div class=grid><section class="panel list"><h2>Trajectories</h2><div id=traces></div></section><main class=panel><div class=playback><button id=previous>&larr;</button><button id=play>Play</button><button id=next>&rarr;</button><input id=position type=range min=0 value=0><span id=step></span><button id=explain>Explain decision</button><button id=modelGraph>Model graph</button><button id=download>Download JSON</button></div><div id=summary></div><div id=relics></div><div id=phase></div><div id=state></div><h2>Legal actions</h2><table class=actions><thead><tr><th>Choice</th><th>Action</th><th>Source</th><th>Target</th><th>Probability</th><th>Log p</th></tr></thead><tbody id=actions></tbody></table><section id=explanation class=explanation hidden><div class=bar><h2>Decision explanation</h2><button id=downloadExplanation>Download explanation JSON</button></div><div id=explanationSummary></div><h3>Causal token deletion</h3><div id=influences></div><h3>Attention <span class=muted>routing, not causal attribution</span></h3><div class=controls><label>Layer <select id=attentionRecord></select></label><label>Head <select id=attentionHead></select></label></div><div id=attentionOverview></div><div id=attentionHeatmap class=attention></div></section><div id=inventory></div><section id=trainingSignal class=training-signal hidden><h2>Loss</h2><div id=signalSummary></div><p id=signalNote class=muted></p><div id=formulaTree></div></section><details><summary>Raw state</summary><pre id=raw></pre></details></main></div>
<script>
const $=id=>document.querySelector('#'+id),names=['Ironclad','Defect','Silent','Regent','Necrobinder'];let listing,current,timer,explanation,assets={},glossary={},content={},terms={};const explanationCache=new Map;
function esc(value){return String(value??'—').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}function value(id){const v=$(id).value;return v===''?undefined:Number(v)}function setStatus(text,error=false){$('status').textContent=text;$('status').className=error?'error':'muted'}
async function request(url,options){const response=await fetch(url,options),text=await response.text();if(!response.ok)throw Error(text.match(/<p>Message: ([^<]+)/)?.[1]||text||response.statusText);return JSON.parse(text)}
async function catalog(){const [runs,map,text]=await Promise.all([request('/api/trajectories'),request('/game-assets/assets.json'),request('/game-assets/glossary.json')]);assets=map;glossary=text;content=text.content;for(const [kind,rows] of Object.entries(glossary))if(kind!=='content')for(const [key,value] of Object.entries(rows))if(key.endsWith('.title'))terms[String(value).toLowerCase()]={kind,key:key.slice(0,-6)};$('run').innerHTML=runs.map(name=>`<option>${esc(name)}</option>`).join('');if(runs.length)await loadRun()}
async function loadRun(){setStatus('loading');listing=await request('/api/trajectories?run='+encodeURIComponent($('run').value));$('checkpoint').innerHTML=listing.checkpoints.map(name=>`<option>${esc(name)}</option>`).join('');$('generate').disabled=!!listing.checkpoint_error;$('generationNote').textContent=listing.checkpoint_error||'';$('session').innerHTML='<option value="">All</option>'+[...new Set(listing.traces.map(t=>t.session_id).filter(v=>v!==null))].map(value=>`<option>${esc(value)}</option>`).join('');for(const [id,key] of [['seed','seed'],['temperature','temperature'],['maxSteps','max_steps'],['maxCombat','max_combat_steps']])$(id).value=listing.defaults[key];renderList();setStatus(`${listing.traces.length} trajectories`)}
function filtered(){const session=$('session').value,character=$('character').value,outcome=$('outcome').value,floor=Number($('floor').value)||0,seed=$('seedFilter').value,revision=$('revision').value;return listing.traces.filter(t=>(!session||t.session_id==session)&&(character===''||t.character==character)&&(!outcome||t.outcome===outcome)&&Number(t.terminal_floor)>=floor&&(!seed||t.seed==seed)&&(!revision||(Number(t.policy_revision_min)<=revision&&Number(t.policy_revision_max)>=revision)))}
function renderList(){const rows=filtered();$('traces').innerHTML=rows.length?rows.map(t=>`<button class=trace data-id="${esc(t.id)}"><b>${esc(names[t.character])} · ${esc(t.outcome)}</b><br><span class=muted>floor ${esc(t.terminal_floor)} · ${t.length} decisions · seed ${esc(t.seed)} · policy ${esc(t.policy_revision_min)}${t.policy_revision_max!==t.policy_revision_min?'–'+esc(t.policy_revision_max):''}</span></button>`).join(''):'<p class=muted>No captured trajectories. Enable <code>--capture-trajectories</code> or generate one above.</p>';document.querySelectorAll('.trace').forEach(node=>node.onclick=()=>openTrace(node.dataset.id))}
async function openTrace(id){try{setStatus('replaying');current=await request(`/api/trajectory?run=${encodeURIComponent($('run').value)}&id=${encodeURIComponent(id)}`);explanation=null;$('explanation').hidden=true;$('position').max=current.steps.length;$('position').value=0;show();document.querySelectorAll('.trace').forEach(node=>node.classList.toggle('active',node.dataset.id===id));setStatus('')}catch(error){setStatus(error.message,true)}}
function label(value){let text=String(value??'—');if(/^(CARD|CHARACTER|MONSTER|RELIC|POTION|POWER|ORB)\./.test(text))text=text.split('.').at(-1).toLowerCase();else if(text.startsWith('EVENT.'))text=text.slice(6).toLowerCase();return text.replaceAll('_',' ')}function details(value){const id=typeof value==='string'?value:value?.id,kind=value?.kind||id?.split('.')[0].toLowerCase(),definition=content[kind+'s']?.[id]||{},key=id?.split('.').at(-1);return{id,kind,key,...definition,...(typeof value==='object'?value:{})}}function title(value){const row=details(value);return glossary[row.kind]?.[row.key+'.title']||label(row.id)}function description(value,smart=true){const row=details(value),table=glossary[row.kind]||{},base=table[row.key+'.description']||'',specific=table[row.key+'.smartDescription']||'';return smart&&row.amount!==undefined&&!/\{(?!Amount[:}])/.test(specific)?specific:base}function resolveText(text,value){const row=details(value),upgrade=row.upgrades>0?2:1,amounts=(row.effects||[]).flatMap(effect=>[...effect.matchAll(/base: (-?\d+), upgraded: (-?\d+)/g)].map(match=>Number(match[upgrade]))),used={};return String(text).replace(/\{([^}:]+)(?::([^}]+))?\}/g,(all,name,format='')=>{let number=name==='Amount'?row.amount:(row.values?.[name]??used[name]??amounts.shift()),direct=(row.effects||[]).join(' ').match(new RegExp(name.replace(/Power$/,'')+'\\((-?\\d+)','i'));if(number===undefined&&direct)number=Number(direct[1]);if(number===undefined)return label(name.replace(/Power$/,''));used[name]=number;if(format.startsWith('plural:')){const words=format.slice(7).split('|');return words[number===1?0:1]||words[0]}if(format.startsWith('cond:')){const words=format.split('?')[1]?.split('|')||[];return words[number<0?0:1]||number}if(format.includes('energyIcons'))return number+' Energy';return format.includes('abs')?Math.abs(number):number})}function plain(text,value){return esc(resolveText(text,value)).replace(/\[(?:\/?)[a-z]+\]/gi,'').replaceAll('\n','<br>')}function keyword(word){const term=terms[word.toLowerCase()];if(!term)return `<span class=accent>${esc(word)}</span>`;const id=term.kind==='keyword'||term.kind==='tip'?null:term.kind.toUpperCase()+'.'+term.key,value=id||{kind:term.kind,id:term.key},text=glossary[term.kind][term.key+'.description']||'';return `<span class=keyword>${esc(word)}<span class=preview>${id?art(value):''}<b>${esc(glossary[term.kind][term.key+'.title']||word)}</b><span class=preview-description>${plain(text,value)}</span></span></span>`}function rich(text,value){return esc(resolveText(text,value)).replace(/\[(?:gold|blue|green|red)\](.*?)\[\/(?:gold|blue|green|red)\]/gi,(_,word)=>keyword(word)).replace(/\[(?:\/?)[a-z]+\]/gi,'').replaceAll('\n','<br>')}function art(value){const row=details(value),sprite=assets[`${row.kind}/${row.key?.toLowerCase()}`];if(!sprite)return'';return `<span class=preview-art role=img aria-label="${esc(title(row))}" style="width:${sprite[3]}px;height:${sprite[4]}px;background-image:url('/game-assets/${esc(sprite[0])}.webp');background-position:-${sprite[1]}px -${sprite[2]}px"></span>`}function fullCard(value){const row=details(value),index=row.upgrades>0?1:0,cost=Array.isArray(row.cost)?row.cost[index]:row.cost,stars=Array.isArray(row.stars)?row.stars[index]:row.stars,type=label(row.type).toLowerCase(),resource=stars>0?'★'.repeat(stars):cost<0?'X':cost;return `<span class="full-card ${esc(type)}">${art(row)}<b class=card-title>${esc(title(row))}${row.upgrades?' +'+row.upgrades:''}</b><b class=card-cost>${esc(resource??'—')}</b><span class=card-description>${rich(description(row),row)}</span><span class=card-type>${esc(type)} · ${esc(label(row.rarity))}</span></span>`}function preview(value){const row=details(value);if(!row.id||!description(row)&&!art(row))return'';if(row.kind==='card')return `<span class="preview card-popup">${fullCard(row)}</span>`;const effects=description(row)?'':(row.effects||[]).map(effect=>`<li>${esc(label(effect))}</li>`).join('');return `<span class=preview>${art(row)}<b>${esc(title(row))}</b>${row.amount!==undefined?` <span class=muted>${row.amount}</span>`:''}${description(row)?`<span class=preview-description>${rich(description(row),row)}</span>`:''}${effects?`<ul>${effects}</ul>`:''}</span>`}function display(value){const row=details(value);return row.id?title(row)+(row.upgrades?' +'+row.upgrades:'')+(row.amount!==undefined?' '+row.amount:''):label(value)}function thing(value,text){const tip=preview(value),name=esc(text??display(value));return tip?`<span class=item>${name}${tip}</span>`:name}function chips(rows){return `<div class=cards>${(rows||[]).map(row=>`<span class=chip>${thing(row)}</span>`).join('')}</div>`}
const spriteAliases={battle_friend_v1_0:'battle_friend_v1',battle_friend_v2_0:'battle_friend_v2',battle_friend_v3_0:'battle_friend_v3',crusher:'kaiser_crab_boss',decimillipede_3_segments:'decimillipede_segment_middle',door:'aeonglass',doormaker:'aeonglass',hatchling_minion:'hatchling',pael_s_legion:'paels_legion',parafright_minion:'parafright',rocket:'architect',stage_1:'test_subject',stage_2:'test_subject',stage_3:'test_subject',test_subject_c_count:'test_subject',the_adversary_mk_1:'the_adversary_mk_one',the_adversary_mk_2:'the_adversary_mk_two',the_adversary_mk_3:'the_adversary_mk_three',the_architect:'architect'};function sprite(value){const row=details(value);row.key=spriteAliases[row.key?.toLowerCase()]||row.key;return art(row)||`<span class="actor-sprite missing-icon">?</span>`}function icon(value,className){const image=art(value);return image?`<span class="${className}" aria-label="${esc(display(value))}">${image}${preview(value)}</span>`:`<span class="${className} missing-icon" title="${esc(display(value))}">?</span>`}function actor(name,row,id,intent){if(!row)return'';const health=Math.max(0,Math.min(100,100*row.hp/Math.max(1,row.max_hp)));return `<section class=actor><div class=actor-heading><b>${esc(title(name))}</b>${intent?`<span>${esc(label(intent))}</span>`:''}</div><div class=actor-sprite>${sprite(id)}</div><div class=vitals><div class=health><span style="width:${health}%"></span><b>${row.hp}/${row.max_hp}</b></div><div class=block><b>${row.block}</b></div></div><div class=effect-line>${(row.powers||[]).map(power=>icon(power,'effect-icon')).join('')}</div></section>`}
function placePreview(event){const source=event.target.closest?.('.action-name,.item,.keyword,.relic-icon,.effect-icon'),tip=source?.querySelector(':scope>.preview');if(!tip)return;requestAnimationFrame(()=>{if(!source.matches(':hover'))return;const anchor=source.getBoundingClientRect(),box=tip.getBoundingClientRect(),margin=8;tip.style.left=Math.min(innerWidth-box.width-margin,Math.max(margin,anchor.left+(anchor.width-box.width)/2))+'px';tip.style.top=(anchor.bottom+box.height<=innerHeight-margin?anchor.bottom:Math.max(margin,anchor.top-box.height))+'px'})}
function actionText(action){const name=display(action.preview||action.label||action.kind);return action.kind.startsWith('RemoveCard(')?`${action.kind} (${name})`:action.kind==='use_potion'?`Drink ${name}`:action.kind==='discard_potion'?`Discard ${name}`:name}function actionThing(action){return thing(action.preview||action.label||action.kind,actionText(action))}
const expandedSignal=new Set;function shown(value){if(typeof value!=='number'||!Number.isFinite(value))return'—';if(value===0)return'0';return value.toExponential(5).replace(/\.?0+e/,'e').replace('e+','e')}function term(id,latex,value,note,definition){return{id,latex,value,note,definition}}function time(offset){return offset?`t+${offset}`:'t'}function nextRow(row){const index=row.training_signal.next_actionable_index;return index===null||index===undefined?null:current.steps[index]}
function potentialTerm(row,offset){const at=time(offset),s=row.training_signal,leaves=s.potential_components.filter(item=>item.weight).map((item,index)=>{const name=item.name.replaceAll('_','\\_'),weight=term(`weight-${row.index}-${index}`,`w_{\\mathrm{${name}}}`,item.weight,'Potential-head weight.'),feature=term(`feature-${row.index}-${index}`,`x_{\\mathrm{${name}}}(s_{${at}})`,item.feature,'Run feature.');return term(`potential-${row.index}-${index}`,`w_{\\mathrm{${name}}}x_{\\mathrm{${name}}}`,item.contribution,'Weighted run feature.',[weight,'\\cdot',feature])});return term(`potential-${row.index}`,`\\Phi(s_{${at}})`,s.potential,'Linear potential head of the critic, at its configured initialization.',leaves.flatMap((item,index)=>index?['+',item]:[item]))}
function rewardTerm(row,offset){const at=time(offset);return term(`reward-${row.index}`,`r_{${at}}`,row.training_signal.reward,'Normalized terminal-floor reward, nonzero only at the last actionable state.')}
function returnTerm(row,offset){const at=time(offset);return term(`return-${row.index}`,`G_{${at}}`,row.training_signal.return,'Discounted critic target.',()=>{const next=nextRow(row),following=next?returnTerm(next,offset+1):term(`return-end-${row.index}`,`G_{t+${offset+1}}`,0,'Zero after the terminal actionable state.');return[rewardTerm(row,offset),'+',term(`gamma-g-${row.index}`,'\\gamma',current.training_config.gae_gamma,'Return discount.'),'\\,',following]})}
function criticTerm(row,offset){const at=time(offset);return term(`value-${row.index}`,`V_\\theta(s_{${at}})`,row.training_signal.critic_value,'Captured critic value.',()=>[term(`network-${row.index}`,`f_\\theta(s_{${at}})`,row.training_signal.network_value,'Network critic output.'),'+',potentialTerm(row,offset)])}
function deltaTerm(row,offset){const at=time(offset),next=nextRow(row),nextValue=next?criticTerm(next,offset+1):term(`value-end-${row.index}`,`V_\\theta(s_{t+${offset+1}})`,0,'Zero after the terminal actionable state.');return term(`delta-${row.index}`,`\\delta_{${at}}`,row.training_signal.td_error,'One-step temporal-difference error.',()=>[rewardTerm(row,offset),'+',term(`gamma-d-${row.index}`,'\\gamma',current.training_config.gae_gamma,'Value discount.'),'\\,',nextValue,'-',criticTerm(row,offset)])}
function advantageTerm(row,offset){const at=time(offset);return term(`advantage-${row.index}`,`A_{${at}}`,row.training_signal.advantage,'Raw generalized advantage estimate.',()=>{const next=nextRow(row),following=next?advantageTerm(next,offset+1):term(`advantage-end-${row.index}`,`A_{t+${offset+1}}`,0,'Zero after the terminal actionable state.');return[deltaTerm(row,offset),'+',term(`gamma-a-${row.index}`,'\\gamma',current.training_config.gae_gamma,'GAE discount.'),'\\,',term(`lambda-${row.index}`,'\\lambda',current.training_config.gae_lambda,'GAE trace decay.'),'\\,',following]})}
function signalTree(row){const s=row.training_signal,c=current.training_config,unknown='Known only when this row is placed in a training batch.',criticValue=criticTerm(row,0);if(!s.eligible)return[criticValue];const returned=returnTerm(row,0),advantage=advantageTerm(row,0),normalized=term('normalized','\\widehat A_t',null,unknown,['\\left(',advantage,'-',term('adv-mean','\\mu_{A,c}',null,'Per-character batch mean; '+unknown),'\\right)/\\left(',term('adv-std','\\sigma_{A,c}+10^{-8}',null,'Per-character batch standard deviation; '+unknown),'\\right)']),oldLog=term('old-log','\\log \\pi_{old}(a_t|s_t)',s.selected_log_probability,'Captured behavior log probability.'),newLog=term('new-log','\\log \\pi_\\theta(a_t|s_t)',null,'Depends on the learner checkpoint used at update time.'),ratio=term('ratio','\\rho_t',null,'Equals 1 at collection; recomputed from the learner policy at training time.',[term('exp','\\exp',null,'Exponential.'),'\\left(',newLog,'-',oldLog,'\\right)']),clipped=term('clipped','\\operatorname{clip}(\\rho_t,1-\\epsilon,1+\\epsilon)',null,unknown,[ratio,',\\quad',term('clip','\\epsilon',c.clip,'PPO clipping radius.')]),policy=term('policy-loss','\\ell_t^{PPO}',null,unknown,['-',term('minimum','\\min',null,'PPO uses the smaller surrogate objective.'),'\\left(',ratio,'\\,',normalized,',\\,',clipped,'\\,',normalized,'\\right)']),square=term('critic-square','(V_\\theta(s_t)-G_t)^2',s.critic_squared_error,'Collection-time squared critic error.',['\\left(',criticValue,'-',returned,'\\right)^2']),critic=term('critic-loss','c_V\\ell_t^V',null,unknown,[term('value-weight','c_V',c.value_weight,'Configured critic-loss weight.'),'\\,',term('balance','w_t',null,'Stateful critic-balance weight. '+unknown),'\\,',square]),entropyTerms=row.actions.map((action,index)=>term(`entropy-${index}`,`-p_${index}\\log p_${index}`,-action.probability*action.log_probability,`${label(action.label||action.kind)} entropy contribution.`)),entropy=term('entropy','H_t',s.entropy,'Entropy of the captured behavior policy.',entropyTerms.flatMap((item,index)=>index?['+',item]:[item])),entropyWeight=term('entropy-weight','c_H',c.entropy_weight,c.entropy_weight===null?'Scheduled from training progress; '+unknown:'Configured entropy weight.'),loss={id:'loss',latex:'\\ell_t',definition:[policy,'+',critic,'-',entropyWeight,'\\,',entropy]};return[loss,criticValue]}
function math(node,latex,className,title){const span=document.createElement(node);latex=latex.replaceAll('\\left','').replaceAll('\\right','');if(className)span.className=className;if(title)span.title=title;if(window.katex)katex.render(latex,span,{throwOnError:false});else span.textContent=latex;return span}function renderFormula(node,depth=0){const wrap=document.createElement('div'),line=document.createElement('div'),definition=typeof node.definition==='function'?node.definition():node.definition||[];line.className='formula-line';line.style.marginLeft=`${depth*18}px`;line.append(math('span',node.latex,'formula-leaf',node.note));line.append(math('span','=','',null));for(const item of definition){if(typeof item==='string'){line.append(math('span',item));continue}const expandable=typeof item.definition==='function'||item.definition?.length,part=math(expandable?'button':'span',item.latex,expandable?'formula-term':'formula-leaf',`${shown(item.value)}${item.note?' · '+item.note:''}`);if(expandable){part.type='button';part.setAttribute('aria-expanded',expandedSignal.has(item.id));part.onclick=()=>{expandedSignal.has(item.id)?expandedSignal.delete(item.id):expandedSignal.add(item.id);renderTraining(current.steps[Number($('position').value)],false)}}line.append(part)}wrap.append(line);const seen=new Set;for(const item of definition)if(typeof item!=='string'&&(typeof item.definition==='function'||item.definition?.length)&&expandedSignal.has(item.id)&&!seen.has(item.id)){seen.add(item.id);wrap.append(renderFormula(item,depth+1))}return wrap}
function renderTraining(row,reset=true){const section=$('trainingSignal');if(!row){section.hidden=true;return}if(reset)expandedSignal.clear();section.hidden=false;const s=row.training_signal,values=[['Critic value',s.critic_value],['Potential head',s.potential],['Reward',s.reward],['Return target',s.return],['Raw advantage',s.advantage]];$('signalSummary').innerHTML=`<div class=signal-values>${values.map(([name,value])=>`<span class=chip><b>${name}</b> ${shown(value)}</span>`).join('')}</div>`;$('signalNote').textContent=s.reason||'Captured values are exact; — marks quantities determined only when a training batch is assembled.';const tree=$('formulaTree');tree.replaceChildren(...signalTree(row).map(node=>renderFormula(node)))}
function phase(state,base,actions){if(state.event){const event=state.event,choices=actions.map(action=>`<li>${actionThing(action)} · ${(100*action.probability).toFixed(2)}%</li>`).join('');return `<section class=phase><h2>Current phase</h2><h3>${esc(label(event.id))}</h3><p>${esc(label(event.page))} screen · ${actions.length} resolved legal choice${actions.length===1?'':'s'}${event.resume?` · returns to ${esc(label(event.resume))}`:''}</p>${choices?`<ol>${choices}</ol>`:''}</section>`}if(state.turn!==undefined)return `<section class=phase><h2>Current phase</h2><h3>Combat · turn ${state.turn}</h3><p>${state.energy}/${state.max_energy} energy · ${state.stars} stars</p></section>`;return `<section class=phase><h2>Current phase</h2><h3>${esc(label(base.phase))}</h3><p>${esc(label(base.room))}</p></section>`}
function show(){if(!current)return;const index=Number($('position').value),terminal=index===current.steps.length,row=terminal?{state:current.final_state}:current.steps[index],state=row.state,base=state.common||state,actions=terminal?[]:[...row.actions].sort((a,b)=>b.probability-a.probability);$('step').textContent=terminal?`Final · ${current.trace.outcome}`:`Decision ${index+1}/${current.steps.length}`;$('summary').innerHTML=`<h2>${esc(label(base.character))} · act ${base.act}, floor ${base.floor}</h2><p>Run HP ${base.hp}/${base.max_hp} · ${base.gold} gold${terminal?'':` · critic ${Number(row.critic_value).toFixed(4)} · entropy ${Number(row.policy_entropy).toFixed(4)} nats (${Number(row.effective_actions).toFixed(2)} effective actions) · progress ${row.canonical_progress} · policy ${row.policy_revision}`}</p>`;$('relics').innerHTML=(base.relics||[]).length?`<div class=relic-line>${base.relics.map(relic=>icon(relic,'relic-icon')).join('')}</div>`:'';$('phase').innerHTML=phase(state,base,actions);const allies=state.turn===undefined?'':actor(base.character,state.player,base.character)+(state.osty?.max_hp?actor('MONSTER.OSTY',state.osty,'MONSTER.OSTY'):'');$('state').innerHTML=state.turn===undefined?'':`<div class=state><div class=combatants><div class=friendlies>${allies}</div><div class=enemies>${(state.enemies||[]).map(enemy=>actor(enemy.id,enemy,enemy.id,enemy.intent)).join('')}</div></div><div class=combat-resources><section><b>Hand</b>${chips(state.hand)}${state.orbs?.length?`<b>Orbs</b>${chips(state.orbs)}`:''}</section><section><b>Draw / discard / exhaust</b><p>${state.draw?.length||0} / ${state.discard?.length||0} / ${state.exhaust?.length||0}</p></section></div></div>`;renderTraining(terminal?null:row);$('raw').textContent=JSON.stringify(state,null,2);$('actions').innerHTML=actions.map(action=>`<tr class="${action.selected?'selected':''}"><td>${action.selected?'✓ ':''}${action.index}</td><td><span class=action-name>${actionThing(action)}</span></td><td>${thing(action.source)}</td><td>${thing(action.target)}</td><td>${(100*action.probability).toFixed(2)}%</td><td>${Number(action.log_probability).toFixed(4)}</td></tr>`).join('');$('inventory').innerHTML=`<section class=inventory><h2>Run inventory</h2><b>Potions</b>${chips(base.potions)}<b>Deck</b>${chips(base.deck)}</section>`;$('previous').disabled=index===0;$('next').disabled=terminal;$('explain').disabled=terminal;$('modelGraph').disabled=terminal||!current.model_graph?.available;$('modelGraph').title=current.model_graph?.reason||'';if(!explanation||explanation.step!==index)$('explanation').hidden=true}
function influenceTable(rows){return `<table class=influences><thead><tr><th>Token</th><th>Status</th><th>Δ margin</th><th>Δ probability</th><th>Δ critic</th><th>Flags</th><th></th></tr></thead><tbody>${rows.map(row=>`<tr><td>${thing(row.label)}</td><td>${esc(label(row.status))}</td><td>${row.margin_delta===undefined?'—':Number(row.margin_delta).toFixed(4)}</td><td>${row.chosen_probability_delta===undefined?'—':Number(row.chosen_probability_delta).toFixed(4)}${row.action_set_changed?' *':''}</td><td>${row.critic_delta===undefined?'—':Number(row.critic_delta).toFixed(4)}</td><td>${esc((row.flags||[]).map(label).join(', ')||row.reason||'')}</td><td>${row.status==='unavailable'?'':`<button class=compare-case data-case="${esc(row.id)}">Attention</button>`}</td></tr>`).join('')}</tbody></table>`}
function showAttention(){const records=explanation._attention||explanation.attention,record=records[Number($('attentionRecord').value)];if(!record)return;const heads=record.weights,head=Number($('attentionHead').value),matrix=head<0?heads[0].map((row,q)=>row.map((_,k)=>heads.reduce((sum,current)=>sum+current[q][k],0)/heads.length)):heads[head],labels=record.labels;const interesting=record.hierarchy==='global_transformer'?[0,...labels.map((token,index)=>token.kind==='action'&&(token.action===explanation.chosen||token.action===explanation.alternative)?index:false).filter(index=>index!==false)]:[0],links=interesting.map(query=>{const top=matrix[query].map((weight,key)=>({weight,key})).sort((a,b)=>b.weight-a.weight).slice(0,8);return `<p><b>${thing(labels[query]?.label||'query '+query)}</b>: ${top.map(item=>`${thing(labels[item.key]?.label||'token '+item.key)} ${(100*item.weight).toFixed(1)}%`).join(' · ')}</p>`}).join('');$('attentionOverview').innerHTML=links;const header=labels.map((token,index)=>`<th title="${esc(label(token.label))}">${index}</th>`).join(''),body=matrix.map((row,query)=>`<tr><th>${thing(labels[query]?.label||'token '+query)}</th>${row.map(weight=>`<td title="${weight.toFixed(6)}" style="background:rgba(96,165,250,${Math.min(1,weight*4)})">${(100*weight).toFixed(1)}</td>`).join('')}</tr>`).join('');$('attentionHeatmap').innerHTML=`<table class=heatmap><thead><tr><th>query \\ key</th>${header}</tr></thead><tbody>${body}</tbody></table>`}
function selectAttention(){const records=explanation._attention||explanation.attention,heads=records[Number($('attentionRecord').value)]?.weights.length||0;$('attentionHead').innerHTML='<option value=-1>Head average</option>'+Array.from({length:heads},(_,index)=>`<option value=${index}>Head ${index}</option>`).join('');showAttention()}function renderExplanation(value){explanation=value;Object.defineProperty(value,'_attention',{value:value.case_attention?[...value.attention.filter(row=>row.hierarchy!=='global_transformer'),...value.case_attention]:value.attention,configurable:true});$('explanation').hidden=false;const baseline=value.baseline,flags=value.flags.map(label).join(', ');$('explanationSummary').innerHTML=`<p>Chosen ${value.chosen} vs alternative ${value.alternative} · margin ${baseline.margin.toFixed(4)} · probabilities ${(100*baseline.chosen_probability).toFixed(2)}% / ${(100*baseline.alternative_probability).toFixed(2)}% · critic ${baseline.critic.toFixed(4)}${flags?' · '+esc(flags):''}${value.attention_case?` · attention after deleting ${esc(label(value.attention_case))}`:''}</p>`;const top=value.influences.slice(0,12);$('influences').innerHTML=influenceTable(top)+(value.influences.length>12?`<details><summary>All ${value.influences.length} results</summary>${influenceTable(value.influences)}</details>`:'');document.querySelectorAll('.compare-case').forEach(node=>node.onclick=()=>analyze(node.dataset.case));$('attentionRecord').innerHTML=value._attention.map((row,index)=>`<option value=${index}>${esc(label(row.hierarchy))} · layer ${row.layer} · call ${row.call}</option>`).join('');selectAttention();scrollTo({top:$('explanation').offsetTop-20,behavior:'smooth'})}
async function analyze(caseId){if(!current||Number($('position').value)>=current.steps.length)return;const step=Number($('position').value),key=`${current.trace.id}:${step}:${caseId||''}`;try{setStatus('analyzing');const body={run:$('run').value,trajectory:current.trace.id,step};if(typeof caseId==='string')body.case=caseId;const value=explanationCache.get(key)||await request('/api/explanation',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});explanationCache.set(key,value);renderExplanation(value);setStatus('')}catch(error){setStatus(error.message,true)}}
async function generate(){try{setStatus('generating');const body={run:$('run').value,checkpoint:$('checkpoint').value,seed:value('seed'),runs:value('runs'),sample:$('mode').value==='sample',temperature:value('temperature'),stage:value('stage'),ascension:value('ascension'),bonus:value('bonus'),max_steps:value('maxSteps'),max_combat_steps:value('maxCombat'),snapshot:$('snapshot').checked,characters:[...document.querySelectorAll('#characters input:checked')].map(node=>Number(node.value))};for(const key of Object.keys(body))if(body[key]===undefined)delete body[key];const result=await request('/api/trajectory',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});await loadRun();if(result.generated.length)await openTrace(result.generated[0].id)}catch(error){setStatus(error.message,true)}}
async function importTrace(file){try{const trace=JSON.parse(await file.text()),result=await request('/api/trajectory',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({run:$('run').value,trace})});await loadRun();current=result;$('position').max=current.steps.length;$('position').value=0;show()}catch(error){setStatus(error.message,true)}}
addEventListener('pointerover',placePreview);
$('characters').innerHTML=names.map((name,index)=>`<label><input type=checkbox value=${index} checked>${name}</label>`).join(' ');$('character').innerHTML+=[...names].map((name,index)=>`<option value=${index}>${name}</option>`).join('');$('run').onchange=loadRun;$('refresh').onclick=loadRun;for(const id of ['session','character','outcome','floor','seedFilter','revision'])$(id).oninput=renderList;$('generate').onclick=generate;$('explain').onclick=()=>analyze();$('modelGraph').onclick=()=>{if(!current||$('modelGraph').disabled)return;const query=new URLSearchParams({run:$('run').value,trajectory:current.trace.id,step:$('position').value});location.href='/model.html?'+query};$('attentionRecord').onchange=selectAttention;$('attentionHead').onchange=showAttention;$('import').onchange=event=>event.target.files[0]&&importTrace(event.target.files[0]);$('position').oninput=show;$('previous').onclick=()=>{$('position').value=Math.max(0,Number($('position').value)-1);show()};$('next').onclick=()=>{$('position').value=Math.min(Number($('position').max),Number($('position').value)+1);show()};$('play').onclick=()=>{if(timer){clearInterval(timer);timer=null;$('play').textContent='Play'}else{timer=setInterval(()=>{if(Number($('position').value)>=Number($('position').max)){$('play').click();return}$('next').click()},500);$('play').textContent='Pause'}};$('download').onclick=()=>{if(!current)return;const link=document.createElement('a');link.href=URL.createObjectURL(new Blob([JSON.stringify(current.trace,null,2)],{type:'application/json'}));link.download=`trajectory-${current.trace.seed}.json`;link.click();URL.revokeObjectURL(link.href)};$('downloadExplanation').onclick=()=>{if(!explanation)return;const link=document.createElement('a');link.href=URL.createObjectURL(new Blob([JSON.stringify(explanation,null,2)],{type:'application/json'}));link.download=`explanation-${current.trace.seed}-${explanation.step}.json`;link.click();URL.revokeObjectURL(link.href)};addEventListener('keydown',event=>{if(event.target.matches('input,select'))return;if(event.key==='ArrowLeft')$('previous').click();if(event.key==='ArrowRight')$('next').click()});catalog().catch(error=>setStatus(error.message,true));
</script>'''
