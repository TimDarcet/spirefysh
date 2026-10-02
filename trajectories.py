import gzip
import json
import math
import threading
import time
import uuid
from pathlib import Path

import torch

import sts2_sim
from explanations import explain as explain_decision
from model import COMPATIBLE_MODEL_VERSIONS, CATEGORIES, MAX_PROGRESS, MODEL_VERSION, \
    POTENTIAL_TERMS, STAGES, export_value_model, load, sha256_file


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
    if trace.get("trajectory_schema") != 1:
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
        "terminal_potential": bool(training.get("terminal_potential", False)),
        "potential_mode": manifest.get("architecture", {}).get(
            "potential_mode", "health_multiplier",
        ),
        "clip": float(training.get("clip", .2)),
        "value_weight": float(training.get("value_weight", .5)),
        "entropy_weight": (float(training["entropy_weight"])
                           if training.get("entropy_weight") is not None else None),
        "boss_floor_increments": [float(training.get(
            f"boss_{boss}_floor_increment", default,
        )) for boss, default in enumerate(BOSS_FLOOR_INCREMENTS, 1)],
        "potential_weights": list(manifest.get("architecture", {}).get(
            "potential_weights", [0.] * len(POTENTIAL_TERMS),
        )),
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
            next_potential = steps[actionable[position + 1]]["potential"] \
                if position + 1 < len(actionable) else steps[-1].get("successor_potential", 0.)
            reward = gamma * next_potential - step["potential"]
            if position + 1 == len(actionable):
                reward += terminal_value
            delta = reward + gamma * next_value - step["critic_value"]
            gae = delta + gamma * gae_lambda * gae
            discounted_return = reward + gamma * discounted_return
            values[index] = {
                "reward": reward, "terminal_reward": terminal_value
                if position + 1 == len(actionable) else 0.,
                "next_actionable_index": actionable[position + 1]
                if position + 1 < len(actionable) else None,
                "next_potential": next_potential, "next_critic_value": next_value,
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
            "state_value": step["critic_value"] + step["potential"],
            "environment_reward": step["environment_reward"],
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
    env.set_potential_weights(config["potential_weights"])
    env.set_potential_mode(config["potential_mode"])
    env.set_terminal_potential(config["terminal_potential"])
    if env.fingerprint() != int(trace["fingerprint"]):
        raise ValueError("trajectory does not match simulator content")
    steps = []
    for index, choice in enumerate(trace["choices"]):
        env.observe_tokens()
        state = json.loads(env.state(0))
        base = state.get("common", state)
        health = max(0., float(state.get("player", base)["hp"]))
        floor, resources = env.potential_components(0)
        factor = health if config["potential_mode"] == "health_multiplier" else 1.
        potential_components = [
            {"name": name, "weight": weight,
             "feature": resource / factor if factor else 0.,
             "weighted_feature": weight * resource / factor if factor else 0.,
             "resource": resource,
             "contribution": weight * resource}
            for name, weight, resource in zip(
                POTENTIAL_TERMS, config["potential_weights"], resources,
            )
        ]
        potential = floor + sum(row["contribution"] for row in potential_components)
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
            "critic_value": trace["critic_values"][index],
            "canonical_progress": trace["canonical_progress"][index],
            "phase": trace["phases"][index],
            "policy_revision": trace["policy_revisions"][index],
            "potential": potential,
            "potential_health": health,
            "potential_floor": floor,
            "potential_components": potential_components,
        })
        reward, done, successor_potential = env.step([choice])
        steps[-1]["environment_reward"] = float(reward[0])
        steps[-1]["done"] = bool(done[0])
        steps[-1]["successor_potential"] = float(successor_potential[0])
    _add_training_signals(trace, steps, config)
    return {"trace": trace, "steps": steps, "final_state": json.loads(env.state(0)),
            "training_config": config}


class TrajectorySource:
    def __init__(self, target):
        self.target = Path(target).resolve()
        self.generated = {}
        self.cache = {}
        self.model = None
        self.explanations = {}
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
                compatible = json.loads(path.read_text()).get("model_version") \
                    in COMPATIBLE_MODEL_VERSIONS
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
            compatible = manifest.get("model_version") in COMPATIBLE_MODEL_VERSIONS
            traces = self._captured(name) | self.generated.get(name, {})
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
            trace = self.generated.get(name, {}).get(trace_id) or self._captured(name).get(trace_id)
            if trace is None:
                raise ValueError("unknown trajectory")
            return replay_trace(trace, _training_config(json.loads(
                (run / "run.json").read_text()
            ), trace))

    def add(self, name, trace):
        with self.lock:
            run = self._run(name)
            trace = dict(validate_trace(trace))
            trace["id"] = f"imported:{uuid.uuid4().hex}"
            trace["source"] = "imported"
            self.generated.setdefault(name, {})[trace["id"]] = trace
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
                    self.generated.setdefault(name, {})[trajectory["id"]] = trajectory
                    generated.append(_metadata(trajectory))
            return {"generated": generated}

    def explain(self, name, trace_id, step, case=None):
        with self.lock:
            trace = self.generated.get(name, {}).get(trace_id)
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
                path = (self._run(name) / checkpoint).resolve()
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
            "trajectory_schema": 1, "id": f"generated:{uuid.uuid4().hex}",
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


def trajectory_page():
    return r'''<!doctype html><meta charset=utf-8><title>Spirefysh trajectories</title><link rel=stylesheet href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css"><script src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script><style>
*{box-sizing:border-box}body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}a{color:#77b7ff}h1,h2,h3{margin:0 0 10px}.bar,.controls,.playback{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin:12px 0}button,select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}button{cursor:pointer}.grid{display:grid;grid-template-columns:minmax(280px,380px) 1fr;gap:14px}.panel{padding:14px;background:#171d28;border:1px solid #303a49;border-radius:10px;min-width:0}.list{max-height:70vh;overflow:auto}.trace{display:block;width:100%;text-align:left;margin:5px 0}.trace.active{border-color:#77b7ff}.muted{color:#9aa8bb}.state{display:grid;grid-template-columns:1fr 1fr;gap:12px}.phase,.inventory,.explanation,.training-signal{margin:18px 0}.cards,.powers,.signal-values{display:flex;gap:5px;flex-wrap:wrap}.chip{padding:4px 7px;background:#252d3a;border-radius:5px}.item,.keyword{position:relative;border-bottom:1px dotted #9aa8bb}.keyword,.accent{color:#efc76b}.actions,.influences,.heatmap{width:100%;border-collapse:collapse}.actions th,.actions td,.influences th,.influences td,.heatmap th,.heatmap td{padding:7px;border-bottom:1px solid #303a49;text-align:left;vertical-align:top}.actions tr.selected{background:#173a2a}.action-name{position:relative;display:inline-block;border-bottom:1px dotted #9aa8bb}.preview{display:none;position:absolute;z-index:10;left:0;top:100%;width:280px;padding:12px;background:#0d1118;border:1px solid #526176;border-radius:7px;box-shadow:0 8px 24px #000;text-align:left;font-weight:400;color:#e8ecf2}.action-name:hover>.preview,.item:hover>.preview,.keyword:hover>.preview{display:block}.preview .keyword>.preview{left:100%;top:-12px}.preview-art{display:block;margin:0 auto 10px;background-repeat:no-repeat}.preview b{font-size:16px}.preview-description{display:block;margin:7px 0;white-space:normal}.card-popup{width:276px;padding:12px}.full-card{display:block;position:relative;width:250px;height:380px;overflow:hidden;border:3px solid #8e7560;border-radius:16px;background:linear-gradient(#602820 0 62%,#261b18 62%);box-shadow:inset 0 0 25px #000}.full-card.skill{background:linear-gradient(#315275 0 62%,#172536 62%)}.full-card.power{background:linear-gradient(#684b78 0 62%,#2d2034 62%)}.full-card.status,.full-card.curse{background:linear-gradient(#5b5b5b 0 62%,#282828 62%)}.full-card .preview-art{position:absolute;top:43px;left:-3px;margin:0}.card-title{position:absolute;z-index:1;top:10px;left:32px;right:8px;text-align:center;text-shadow:0 2px 3px #000}.card-cost{position:absolute;z-index:2;top:7px;left:6px;width:31px;height:31px;padding-top:5px;border-radius:50%;background:#315cba;text-align:center;font-size:18px}.card-description{position:absolute;top:242px;left:12px;right:12px;bottom:27px;display:flex;align-items:center;justify-content:center;overflow:auto;text-align:center;font-size:12px;line-height:1.35}.card-type{position:absolute;bottom:7px;left:10px;right:10px;text-align:center;color:#c9bba9;font-size:11px}.attention{overflow:auto;max-height:600px}.heatmap td{min-width:38px;text-align:center}.signal-values .chip b{color:#77b7ff}.formula-line{display:flex;align-items:center;flex-wrap:wrap;gap:7px;min-height:42px;border-left:2px solid #303a49;padding-left:12px}.formula-term{border:0;border-bottom:1px dotted #77b7ff;border-radius:0;background:transparent;color:#e8ecf2;padding:2px}.formula-term[aria-expanded=true]{color:#77b7ff}.formula-leaf{display:inline-block;border-bottom:1px dotted #9aa8bb;padding:2px}.error{color:#ee7b7b}pre{white-space:pre-wrap;word-break:break-word}input[type=range]{flex:1;min-width:240px}@media(max-width:900px){.grid,.state{grid-template-columns:1fr}}
</style><div class=bar><a href=/>&larr; Metrics</a><h1>Trajectories</h1><span id=status class=muted></span></div>
<div class=panel><div class=controls><label>Run <select id=run></select></label><label>Session <select id=session><option value="">All</option></select></label><label>Character <select id=character><option value="">All</option></select></label><label>Outcome <select id=outcome><option value="">All</option><option>won</option><option>dead</option><option>step_cap</option><option>combat_cap</option><option>empty_actions</option></select></label><label>Minimum floor <input id=floor type=number min=0 value=0></label><label>Seed <input id=seedFilter type=number placeholder=any></label><label>Policy revision <input id=revision type=number placeholder=any></label><button id=refresh>Refresh</button><label>Import <input id=import type=file accept=application/json></label></div>
<details><summary>Generate from checkpoint</summary><div class=controls><label>Checkpoint <select id=checkpoint></select></label><label>Seed <input id=seed type=number></label><label>Runs / character <input id=runs type=number min=1 max=64 value=1></label><label>Mode <select id=mode><option value=sample>Sampled</option><option value=greedy>Greedy</option></select></label><label>Temperature <input id=temperature type=number min=.001 step=.05></label><label>Stage <input id=stage type=number min=0 placeholder=checkpoint></label><label>Ascension <input id=ascension type=number min=0 placeholder=stage></label><label>Bonus <input id=bonus type=number min=0 placeholder=stage></label><label>Max steps <input id=maxSteps type=number min=1></label><label>Max combat steps <input id=maxCombat type=number min=1></label><span id=characters></span><button id=generate>Generate</button><span id=generationNote class=error></span></div></details></div>
<div class=grid><section class="panel list"><h2>Trajectories</h2><div id=traces></div></section><main class=panel><div class=playback><button id=previous>&larr;</button><button id=play>Play</button><button id=next>&rarr;</button><input id=position type=range min=0 value=0><span id=step></span><button id=explain>Explain decision</button><button id=download>Download JSON</button></div><div id=summary></div><div id=phase></div><div id=state></div><section id=trainingSignal class=training-signal hidden><h2>What the training signal would be if this was a training example</h2><div id=signalSummary></div><p id=signalNote class=muted></p><div id=formulaTree></div></section><h2>Legal actions</h2><table class=actions><thead><tr><th>Choice</th><th>Action</th><th>Source</th><th>Target</th><th>Probability</th><th>Log p</th></tr></thead><tbody id=actions></tbody></table><section id=explanation class=explanation hidden><div class=bar><h2>Decision explanation</h2><button id=downloadExplanation>Download explanation JSON</button></div><div id=explanationSummary></div><h3>Causal token deletion</h3><div id=influences></div><h3>Attention <span class=muted>routing, not causal attribution</span></h3><div class=controls><label>Layer <select id=attentionRecord></select></label><label>Head <select id=attentionHead></select></label></div><div id=attentionOverview></div><div id=attentionHeatmap class=attention></div></section><div id=inventory></div><details><summary>Raw state</summary><pre id=raw></pre></details></main></div>
<script>
const $=id=>document.querySelector('#'+id),names=['Ironclad','Defect','Silent','Regent','Necrobinder'];let listing,current,timer,explanation,assets={},glossary={},content={},terms={};const explanationCache=new Map;
function esc(value){return String(value??'—').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}function value(id){const v=$(id).value;return v===''?undefined:Number(v)}function setStatus(text,error=false){$('status').textContent=text;$('status').className=error?'error':'muted'}
async function request(url,options){const response=await fetch(url,options),text=await response.text();if(!response.ok)throw Error(text.match(/<p>Message: ([^<]+)/)?.[1]||text||response.statusText);return JSON.parse(text)}
async function catalog(){const [runs,map,text]=await Promise.all([request('/api/trajectories'),request('/game-assets/assets.json'),request('/game-assets/glossary.json')]);assets=map;glossary=text;content=text.content;for(const [kind,rows] of Object.entries(glossary))if(kind!=='content')for(const [key,value] of Object.entries(rows))if(key.endsWith('.title'))terms[String(value).toLowerCase()]={kind,key:key.slice(0,-6)};$('run').innerHTML=runs.map(name=>`<option>${esc(name)}</option>`).join('');if(runs.length)await loadRun()}
async function loadRun(){setStatus('loading');listing=await request('/api/trajectories?run='+encodeURIComponent($('run').value));$('checkpoint').innerHTML=listing.checkpoints.map(name=>`<option>${esc(name)}</option>`).join('');$('generate').disabled=!!listing.checkpoint_error;$('generationNote').textContent=listing.checkpoint_error||'';$('session').innerHTML='<option value="">All</option>'+[...new Set(listing.traces.map(t=>t.session_id).filter(v=>v!==null))].map(value=>`<option>${esc(value)}</option>`).join('');for(const [id,key] of [['seed','seed'],['temperature','temperature'],['maxSteps','max_steps'],['maxCombat','max_combat_steps']])$(id).value=listing.defaults[key];renderList();setStatus(`${listing.traces.length} trajectories`)}
function filtered(){const session=$('session').value,character=$('character').value,outcome=$('outcome').value,floor=Number($('floor').value)||0,seed=$('seedFilter').value,revision=$('revision').value;return listing.traces.filter(t=>(!session||t.session_id==session)&&(character===''||t.character==character)&&(!outcome||t.outcome===outcome)&&Number(t.terminal_floor)>=floor&&(!seed||t.seed==seed)&&(!revision||(Number(t.policy_revision_min)<=revision&&Number(t.policy_revision_max)>=revision)))}
function renderList(){const rows=filtered();$('traces').innerHTML=rows.length?rows.map(t=>`<button class=trace data-id="${esc(t.id)}"><b>${esc(names[t.character])} · ${esc(t.outcome)}</b><br><span class=muted>floor ${esc(t.terminal_floor)} · ${t.length} decisions · seed ${esc(t.seed)} · policy ${esc(t.policy_revision_min)}${t.policy_revision_max!==t.policy_revision_min?'–'+esc(t.policy_revision_max):''}</span></button>`).join(''):'<p class=muted>No captured trajectories. Enable <code>--capture-trajectories</code> or generate one above.</p>';document.querySelectorAll('.trace').forEach(node=>node.onclick=()=>openTrace(node.dataset.id))}
async function openTrace(id){try{setStatus('replaying');current=await request(`/api/trajectory?run=${encodeURIComponent($('run').value)}&id=${encodeURIComponent(id)}`);explanation=null;$('explanation').hidden=true;$('position').max=current.steps.length;$('position').value=0;show();document.querySelectorAll('.trace').forEach(node=>node.classList.toggle('active',node.dataset.id===id));setStatus('')}catch(error){setStatus(error.message,true)}}
function label(value){let text=String(value??'—');if(/^(CARD|RELIC|POTION|POWER|ORB)\./.test(text))text=text.split('.').at(-1).toLowerCase();else if(text.startsWith('EVENT.'))text=text.slice(6).toLowerCase();return text.replaceAll('_',' ')}function details(value){const id=typeof value==='string'?value:value?.id,kind=value?.kind||id?.split('.')[0].toLowerCase(),definition=content[kind+'s']?.[id]||{},key=id?.split('.').at(-1);return{id,kind,key,...definition,...(typeof value==='object'?value:{})}}function title(value){const row=details(value);return glossary[row.kind]?.[row.key+'.title']||label(row.id)}function description(value,smart=true){const row=details(value),table=glossary[row.kind]||{},base=table[row.key+'.description']||'',specific=table[row.key+'.smartDescription']||'';return smart&&row.amount!==undefined&&!/\{(?!Amount[:}])/.test(specific)?specific:base}function resolveText(text,value){const row=details(value),upgrade=row.upgrades>0?2:1,amounts=(row.effects||[]).flatMap(effect=>[...effect.matchAll(/base: (-?\d+), upgraded: (-?\d+)/g)].map(match=>Number(match[upgrade]))),used={};return String(text).replace(/\{([^}:]+)(?::([^}]+))?\}/g,(all,name,format='')=>{let number=name==='Amount'?row.amount:(row.values?.[name]??used[name]??amounts.shift()),direct=(row.effects||[]).join(' ').match(new RegExp(name.replace(/Power$/,'')+'\\((-?\\d+)','i'));if(number===undefined&&direct)number=Number(direct[1]);if(number===undefined)return label(name.replace(/Power$/,''));used[name]=number;if(format.startsWith('plural:')){const words=format.slice(7).split('|');return words[number===1?0:1]||words[0]}if(format.startsWith('cond:')){const words=format.split('?')[1]?.split('|')||[];return words[number<0?0:1]||number}if(format.includes('energyIcons'))return number+' Energy';return format.includes('abs')?Math.abs(number):number})}function plain(text,value){return esc(resolveText(text,value)).replace(/\[(?:\/?)[a-z]+\]/gi,'').replaceAll('\n','<br>')}function keyword(word){const term=terms[word.toLowerCase()];if(!term)return `<span class=accent>${esc(word)}</span>`;const id=term.kind==='keyword'||term.kind==='tip'?null:term.kind.toUpperCase()+'.'+term.key,value=id||{kind:term.kind,id:term.key},text=glossary[term.kind][term.key+'.description']||'';return `<span class=keyword>${esc(word)}<span class=preview>${id?art(value):''}<b>${esc(glossary[term.kind][term.key+'.title']||word)}</b><span class=preview-description>${plain(text,value)}</span></span></span>`}function rich(text,value){return esc(resolveText(text,value)).replace(/\[(?:gold|blue|green|red)\](.*?)\[\/(?:gold|blue|green|red)\]/gi,(_,word)=>keyword(word)).replace(/\[(?:\/?)[a-z]+\]/gi,'').replaceAll('\n','<br>')}function art(value){const row=details(value),sprite=assets[`${row.kind}/${row.key?.toLowerCase()}`];if(!sprite)return'';return `<span class=preview-art role=img aria-label="${esc(title(row))}" style="width:${sprite[3]}px;height:${sprite[4]}px;background-image:url('/game-assets/${esc(sprite[0])}.webp');background-position:-${sprite[1]}px -${sprite[2]}px"></span>`}function fullCard(value){const row=details(value),index=row.upgrades>0?1:0,cost=Array.isArray(row.cost)?row.cost[index]:row.cost,stars=Array.isArray(row.stars)?row.stars[index]:row.stars,type=label(row.type).toLowerCase(),resource=stars>0?'★'.repeat(stars):cost<0?'X':cost;return `<span class="full-card ${esc(type)}">${art(row)}<b class=card-title>${esc(title(row))}${row.upgrades?' +'+row.upgrades:''}</b><b class=card-cost>${esc(resource??'—')}</b><span class=card-description>${rich(description(row),row)}</span><span class=card-type>${esc(type)} · ${esc(label(row.rarity))}</span></span>`}function preview(value){const row=details(value);if(!row.id||!description(row)&&!art(row))return'';if(row.kind==='card')return `<span class="preview card-popup">${fullCard(row)}</span>`;const effects=description(row)?'':(row.effects||[]).map(effect=>`<li>${esc(label(effect))}</li>`).join('');return `<span class=preview>${art(row)}<b>${esc(title(row))}</b>${row.amount!==undefined?` <span class=muted>${row.amount}</span>`:''}${description(row)?`<span class=preview-description>${rich(description(row),row)}</span>`:''}${effects?`<ul>${effects}</ul>`:''}</span>`}function display(value){const row=details(value);return row.id?title(row)+(row.upgrades?' +'+row.upgrades:'')+(row.amount!==undefined?' '+row.amount:''):label(value)}function thing(value,text){const tip=preview(value),name=esc(text??display(value));return tip?`<span class=item>${name}${tip}</span>`:name}function chips(rows){return `<div class=cards>${(rows||[]).map(row=>`<span class=chip>${thing(row)}</span>`).join('')}</div>`}function actor(name,row){return row?`<div><b>${esc(label(name))}</b> ${row.hp}/${row.max_hp} hp · ${row.block} block ${chips(row.powers)}</div>`:''}
function actionText(action){const name=display(action.preview||action.label||action.kind);return action.kind.startsWith('RemoveCard(')?`${action.kind} (${name})`:action.kind==='use_potion'?`Drink ${name}`:action.kind==='discard_potion'?`Discard ${name}`:name}function actionThing(action){return thing(action.preview||action.label||action.kind,actionText(action))}
const expandedSignal=new Set;function shown(value){if(typeof value!=='number'||!Number.isFinite(value))return'—';if(value===0)return'0';return value.toExponential(5).replace(/\.?0+e/,'e').replace('e+','e')}function term(id,latex,value,note,definition){return{id,latex,value,note,definition}}function time(offset){return offset?`t+${offset}`:'t'}function nextRow(row){const index=row.training_signal.next_actionable_index;return index===null||index===undefined?null:current.steps[index]}
function potentialTerm(row,offset){const at=time(offset),s=row.training_signal,multiplied=current.training_config.potential_mode==='health_multiplier',leaves=(s.potential_components||[]).map((item,index)=>{const name=item.name.replaceAll('_','\\_'),weight=term(`weight-${row.index}-${index}`,`w_{\\mathrm{${name}}}`,item.weight,'Configured potential weight.'),feature=term(`feature-${row.index}-${index}`,`x_{\\mathrm{${name}}}(s_{${at}})`,item.feature,'Raw resource feature.');return term(`potential-${row.index}-${index}`,`w_{\\mathrm{${name}}}x_{\\mathrm{${name}}}`,item.weighted_feature,'Weighted potential term.',[weight,'\\cdot',feature])}),resources=term(`resources-${row.index}`,'\\sum_k w_kx_k',leaves.reduce((sum,item)=>sum+item.value,0),'Weighted resource sum.',leaves.flatMap((item,index)=>index?['+',item]:[item])),floor=term(`floor-${row.index}`,`F(s_{${at}})/(C-1)`,row.potential_floor,'Normalized canonical progress.');return term(`potential-${row.index}`,`\\Phi(s_{${at}})`,s.potential,'Fixed progress and resource potential.',multiplied?[floor,'+',term(`health-${row.index}`,`\\mathrm{HP}(s_{${at}})`,row.potential_health,'Current combat or run health.'),'\\,',resources]:[floor,'+',resources])}
function rewardTerm(row,offset){const at=time(offset),s=row.training_signal,next=nextRow(row),nextPotential=next?potentialTerm(next,offset+1):term(`potential-end-${row.index}`,`\\Phi(s_{t+${offset+1}})`,s.next_potential,'Terminal successor potential.');return term(`reward-${row.index}`,`r_{${at}}`,s.reward,'Potential-shaped learner reward.',()=>[term(`gamma-r-${row.index}`,'\\gamma',current.training_config.gae_gamma,'Reward discount.'),'\\,',nextPotential,'-',potentialTerm(row,offset),'+',term(`terminal-${row.index}`,`T_{${at}}`,s.terminal_reward,'Normalized terminal-floor reward, nonzero only at the last actionable state.')])}
function returnTerm(row,offset){const at=time(offset);return term(`return-${row.index}`,`G_{${at}}`,row.training_signal.return,'Discounted critic target.',()=>{const next=nextRow(row),following=next?returnTerm(next,offset+1):term(`return-end-${row.index}`,`G_{t+${offset+1}}`,0,'Zero after the terminal actionable state.');return[rewardTerm(row,offset),'+',term(`gamma-g-${row.index}`,'\\gamma',current.training_config.gae_gamma,'Return discount.'),'\\,',following]})}
function criticTerm(row,offset){return term(`value-${row.index}`,`v_\\theta(s_{${time(offset)}})`,row.training_signal.critic_value,'Captured trainable critic residual.')}
function deltaTerm(row,offset){const at=time(offset),next=nextRow(row),nextValue=next?criticTerm(next,offset+1):term(`value-end-${row.index}`,`v_\\theta(s_{t+${offset+1}})`,0,'Zero after the terminal actionable state.');return term(`delta-${row.index}`,`\\delta_{${at}}`,row.training_signal.td_error,'One-step temporal-difference error.',()=>[rewardTerm(row,offset),'+',term(`gamma-d-${row.index}`,'\\gamma',current.training_config.gae_gamma,'Value discount.'),'\\,',nextValue,'-',criticTerm(row,offset)])}
function advantageTerm(row,offset){const at=time(offset);return term(`advantage-${row.index}`,`A_{${at}}`,row.training_signal.advantage,'Raw generalized advantage estimate.',()=>{const next=nextRow(row),following=next?advantageTerm(next,offset+1):term(`advantage-end-${row.index}`,`A_{t+${offset+1}}`,0,'Zero after the terminal actionable state.');return[deltaTerm(row,offset),'+',term(`gamma-a-${row.index}`,'\\gamma',current.training_config.gae_gamma,'GAE discount.'),'\\,',term(`lambda-${row.index}`,'\\lambda',current.training_config.gae_lambda,'GAE trace decay.'),'\\,',following]})}
function signalTree(row){const s=row.training_signal,c=current.training_config,unknown='Known only when this row is placed in a training batch.',potential=potentialTerm(row,0),criticValue=criticTerm(row,0),stateValue=term(`state-value-${row.index}`,'\\hat V(s_t)',s.state_value,'Total collection-time state value.',[criticValue,'+',potential]);if(!s.eligible)return[stateValue];const returned=returnTerm(row,0),advantage=advantageTerm(row,0),normalized=term('normalized','\\widehat A_t',null,unknown,['\\left(',advantage,'-',term('adv-mean','\\mu_{A,c}',null,'Per-character batch mean; '+unknown),'\\right)/\\left(',term('adv-std','\\sigma_{A,c}+10^{-8}',null,'Per-character batch standard deviation; '+unknown),'\\right)']),oldLog=term('old-log','\\log \\pi_{old}(a_t|s_t)',s.selected_log_probability,'Captured behavior log probability.'),newLog=term('new-log','\\log \\pi_\\theta(a_t|s_t)',null,'Depends on the learner checkpoint used at update time.'),ratio=term('ratio','\\rho_t',null,'Equals 1 at collection; recomputed from the learner policy at training time.',[term('exp','\\exp',null,'Exponential.'),'\\left(',newLog,'-',oldLog,'\\right)']),clipped=term('clipped','\\operatorname{clip}(\\rho_t,1-\\epsilon,1+\\epsilon)',null,unknown,[ratio,',\\quad',term('clip','\\epsilon',c.clip,'PPO clipping radius.')]),policy=term('policy-loss','\\ell_t^{PPO}',null,unknown,['-',term('minimum','\\min',null,'PPO uses the smaller surrogate objective.'),'\\left(',ratio,'\\,',normalized,',\\,',clipped,'\\,',normalized,'\\right)']),square=term('critic-square','(v_\\theta(s_t)-G_t)^2',s.critic_squared_error,'Collection-time squared critic error.',['\\left(',criticValue,'-',returned,'\\right)^2']),critic=term('critic-loss','c_V\\ell_t^V',null,unknown,[term('value-weight','c_V',c.value_weight,'Configured critic-loss weight.'),'\\,',term('balance','w_t',null,'Stateful critic-balance weight. '+unknown),'\\,',square]),entropyTerms=row.actions.map((action,index)=>term(`entropy-${index}`,`-p_${index}\\log p_${index}`,-action.probability*action.log_probability,`${label(action.label||action.kind)} entropy contribution.`)),entropy=term('entropy','H_t',s.entropy,'Entropy of the captured behavior policy.',entropyTerms.flatMap((item,index)=>index?['+',item]:[item])),entropyWeight=term('entropy-weight','c_H',c.entropy_weight,c.entropy_weight===null?'Scheduled from training progress; '+unknown:'Configured entropy weight.'),loss={id:'loss',latex:'\\ell_t',definition:[policy,'+',critic,'-',entropyWeight,'\\,',entropy]};return[loss,stateValue]}
function math(node,latex,className,title){const span=document.createElement(node);latex=latex.replaceAll('\\left','').replaceAll('\\right','');if(className)span.className=className;if(title)span.title=title;if(window.katex)katex.render(latex,span,{throwOnError:false});else span.textContent=latex;return span}function renderFormula(node,depth=0){const wrap=document.createElement('div'),line=document.createElement('div'),definition=typeof node.definition==='function'?node.definition():node.definition||[];line.className='formula-line';line.style.marginLeft=`${depth*18}px`;line.append(math('span',node.latex,'formula-leaf',node.note));line.append(math('span','=','',null));for(const item of definition){if(typeof item==='string'){line.append(math('span',item));continue}const expandable=typeof item.definition==='function'||item.definition?.length,part=math(expandable?'button':'span',item.latex,expandable?'formula-term':'formula-leaf',`${shown(item.value)}${item.note?' · '+item.note:''}`);if(expandable){part.type='button';part.setAttribute('aria-expanded',expandedSignal.has(item.id));part.onclick=()=>{expandedSignal.has(item.id)?expandedSignal.delete(item.id):expandedSignal.add(item.id);renderTraining(current.steps[Number($('position').value)],false)}}line.append(part)}wrap.append(line);const seen=new Set;for(const item of definition)if(typeof item!=='string'&&(typeof item.definition==='function'||item.definition?.length)&&expandedSignal.has(item.id)&&!seen.has(item.id)){seen.add(item.id);wrap.append(renderFormula(item,depth+1))}return wrap}
function renderTraining(row,reset=true){const section=$('trainingSignal');if(!row){section.hidden=true;return}if(reset)expandedSignal.clear();section.hidden=false;const s=row.training_signal,values=[['Total state value',s.state_value],['Critic residual',s.critic_value],['Environment reward',s.environment_reward],['Shaped reward',s.reward],['Return target',s.return],['Raw advantage',s.advantage]];$('signalSummary').innerHTML=`<div class=signal-values>${values.map(([name,value])=>`<span class=chip><b>${name}</b> ${shown(value)}</span>`).join('')}</div>`;$('signalNote').textContent=s.reason||'Captured values are exact; — marks quantities determined only when a training batch is assembled.';const tree=$('formulaTree');tree.replaceChildren(...signalTree(row).map(node=>renderFormula(node)))}
function phase(state,base,actions){if(state.event){const event=state.event,choices=actions.map(action=>`<li>${actionThing(action)} · ${(100*action.probability).toFixed(2)}%</li>`).join('');return `<section class=phase><h2>Current phase</h2><h3>${esc(label(event.id))}</h3><p>${esc(label(event.page))} screen · ${actions.length} resolved legal choice${actions.length===1?'':'s'}${event.resume?` · returns to ${esc(label(event.resume))}`:''}</p>${choices?`<ol>${choices}</ol>`:''}</section>`}if(state.turn!==undefined)return `<section class=phase><h2>Current phase</h2><h3>Combat · turn ${state.turn}</h3><p>${state.energy}/${state.max_energy} energy · ${state.stars} stars</p></section>`;return `<section class=phase><h2>Current phase</h2><h3>${esc(label(base.phase))}</h3><p>${esc(label(base.room))}</p></section>`}
function show(){if(!current)return;const index=Number($('position').value),terminal=index===current.steps.length,row=terminal?{state:current.final_state}:current.steps[index],state=row.state,base=state.common||state,actions=terminal?[]:[...row.actions].sort((a,b)=>b.probability-a.probability);$('step').textContent=terminal?`Final · ${current.trace.outcome}`:`Decision ${index+1}/${current.steps.length}`;$('summary').innerHTML=`<h2>${esc(label(base.character))} · act ${base.act}, floor ${base.floor}</h2><p>Run HP ${base.hp}/${base.max_hp} · ${base.gold} gold${terminal?'':` · critic ${Number(row.critic_value).toFixed(4)} · entropy ${Number(row.policy_entropy).toFixed(4)} nats (${Number(row.effective_actions).toFixed(2)} effective actions) · progress ${row.canonical_progress} · policy ${row.policy_revision}`}</p>`;$('phase').innerHTML=phase(state,base,actions);$('state').innerHTML=state.turn===undefined?'':`<div class=state><section>${actor('Player',state.player)}${actor('Osty',state.osty)}${(state.enemies||[]).map(enemy=>actor(enemy.id+' · '+enemy.intent,enemy)).join('')}</section><section><b>Hand</b>${chips(state.hand)}${state.orbs?.length?`<b>Orbs</b>${chips(state.orbs)}`:''}<b>Draw / discard / exhaust</b><p>${state.draw?.length||0} / ${state.discard?.length||0} / ${state.exhaust?.length||0}</p></section></div>`;renderTraining(terminal?null:row);$('raw').textContent=JSON.stringify(state,null,2);$('actions').innerHTML=actions.map(action=>`<tr class="${action.selected?'selected':''}"><td>${action.selected?'✓ ':''}${action.index}</td><td><span class=action-name>${actionThing(action)}</span></td><td>${thing(action.source)}</td><td>${thing(action.target)}</td><td>${(100*action.probability).toFixed(2)}%</td><td>${Number(action.log_probability).toFixed(4)}</td></tr>`).join('');$('inventory').innerHTML=`<section class=inventory><h2>Run inventory</h2><b>Relics</b>${chips(base.relics)}<b>Potions</b>${chips(base.potions)}<b>Deck</b>${chips(base.deck)}</section>`;$('previous').disabled=index===0;$('next').disabled=terminal;$('explain').disabled=terminal;if(!explanation||explanation.step!==index)$('explanation').hidden=true}
function influenceTable(rows){return `<table class=influences><thead><tr><th>Token</th><th>Status</th><th>Δ margin</th><th>Δ probability</th><th>Δ critic</th><th>Flags</th><th></th></tr></thead><tbody>${rows.map(row=>`<tr><td>${thing(row.label)}</td><td>${esc(label(row.status))}</td><td>${row.margin_delta===undefined?'—':Number(row.margin_delta).toFixed(4)}</td><td>${row.chosen_probability_delta===undefined?'—':Number(row.chosen_probability_delta).toFixed(4)}${row.action_set_changed?' *':''}</td><td>${row.critic_delta===undefined?'—':Number(row.critic_delta).toFixed(4)}</td><td>${esc((row.flags||[]).map(label).join(', ')||row.reason||'')}</td><td>${row.status==='unavailable'?'':`<button class=compare-case data-case="${esc(row.id)}">Attention</button>`}</td></tr>`).join('')}</tbody></table>`}
function showAttention(){const records=explanation._attention||explanation.attention,record=records[Number($('attentionRecord').value)];if(!record)return;const heads=record.weights,head=Number($('attentionHead').value),matrix=head<0?heads[0].map((row,q)=>row.map((_,k)=>heads.reduce((sum,current)=>sum+current[q][k],0)/heads.length)):heads[head],labels=record.labels;const interesting=record.hierarchy==='global_transformer'?[0,...labels.map((token,index)=>token.kind==='action'&&(token.action===explanation.chosen||token.action===explanation.alternative)?index:false).filter(index=>index!==false)]:[0],links=interesting.map(query=>{const top=matrix[query].map((weight,key)=>({weight,key})).sort((a,b)=>b.weight-a.weight).slice(0,8);return `<p><b>${thing(labels[query]?.label||'query '+query)}</b>: ${top.map(item=>`${thing(labels[item.key]?.label||'token '+item.key)} ${(100*item.weight).toFixed(1)}%`).join(' · ')}</p>`}).join('');$('attentionOverview').innerHTML=links;const header=labels.map((token,index)=>`<th title="${esc(label(token.label))}">${index}</th>`).join(''),body=matrix.map((row,query)=>`<tr><th>${thing(labels[query]?.label||'token '+query)}</th>${row.map(weight=>`<td title="${weight.toFixed(6)}" style="background:rgba(96,165,250,${Math.min(1,weight*4)})">${(100*weight).toFixed(1)}</td>`).join('')}</tr>`).join('');$('attentionHeatmap').innerHTML=`<table class=heatmap><thead><tr><th>query \\ key</th>${header}</tr></thead><tbody>${body}</tbody></table>`}
function selectAttention(){const records=explanation._attention||explanation.attention,heads=records[Number($('attentionRecord').value)]?.weights.length||0;$('attentionHead').innerHTML='<option value=-1>Head average</option>'+Array.from({length:heads},(_,index)=>`<option value=${index}>Head ${index}</option>`).join('');showAttention()}function renderExplanation(value){explanation=value;Object.defineProperty(value,'_attention',{value:value.case_attention?[...value.attention.filter(row=>row.hierarchy!=='global_transformer'),...value.case_attention]:value.attention,configurable:true});$('explanation').hidden=false;const baseline=value.baseline,flags=value.flags.map(label).join(', ');$('explanationSummary').innerHTML=`<p>Chosen ${value.chosen} vs alternative ${value.alternative} · margin ${baseline.margin.toFixed(4)} · probabilities ${(100*baseline.chosen_probability).toFixed(2)}% / ${(100*baseline.alternative_probability).toFixed(2)}% · critic ${baseline.critic.toFixed(4)}${flags?' · '+esc(flags):''}${value.attention_case?` · attention after deleting ${esc(label(value.attention_case))}`:''}</p>`;const top=value.influences.slice(0,12);$('influences').innerHTML=influenceTable(top)+(value.influences.length>12?`<details><summary>All ${value.influences.length} results</summary>${influenceTable(value.influences)}</details>`:'');document.querySelectorAll('.compare-case').forEach(node=>node.onclick=()=>analyze(node.dataset.case));$('attentionRecord').innerHTML=value._attention.map((row,index)=>`<option value=${index}>${esc(label(row.hierarchy))} · layer ${row.layer} · call ${row.call}</option>`).join('');selectAttention();scrollTo({top:$('explanation').offsetTop-20,behavior:'smooth'})}
async function analyze(caseId){if(!current||Number($('position').value)>=current.steps.length)return;const step=Number($('position').value),key=`${current.trace.id}:${step}:${caseId||''}`;try{setStatus('analyzing');const body={run:$('run').value,trajectory:current.trace.id,step};if(typeof caseId==='string')body.case=caseId;const value=explanationCache.get(key)||await request('/api/explanation',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});explanationCache.set(key,value);renderExplanation(value);setStatus('')}catch(error){setStatus(error.message,true)}}
async function generate(){try{setStatus('generating');const body={run:$('run').value,checkpoint:$('checkpoint').value,seed:value('seed'),runs:value('runs'),sample:$('mode').value==='sample',temperature:value('temperature'),stage:value('stage'),ascension:value('ascension'),bonus:value('bonus'),max_steps:value('maxSteps'),max_combat_steps:value('maxCombat'),characters:[...document.querySelectorAll('#characters input:checked')].map(node=>Number(node.value))};for(const key of Object.keys(body))if(body[key]===undefined)delete body[key];const result=await request('/api/trajectory',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});await loadRun();if(result.generated.length)await openTrace(result.generated[0].id)}catch(error){setStatus(error.message,true)}}
async function importTrace(file){try{const trace=JSON.parse(await file.text()),result=await request('/api/trajectory',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({run:$('run').value,trace})});await loadRun();current=result;$('position').max=current.steps.length;$('position').value=0;show()}catch(error){setStatus(error.message,true)}}
$('characters').innerHTML=names.map((name,index)=>`<label><input type=checkbox value=${index} checked>${name}</label>`).join(' ');$('character').innerHTML+=[...names].map((name,index)=>`<option value=${index}>${name}</option>`).join('');$('run').onchange=loadRun;$('refresh').onclick=loadRun;for(const id of ['session','character','outcome','floor','seedFilter','revision'])$(id).oninput=renderList;$('generate').onclick=generate;$('explain').onclick=()=>analyze();$('attentionRecord').onchange=selectAttention;$('attentionHead').onchange=showAttention;$('import').onchange=event=>event.target.files[0]&&importTrace(event.target.files[0]);$('position').oninput=show;$('previous').onclick=()=>{$('position').value=Math.max(0,Number($('position').value)-1);show()};$('next').onclick=()=>{$('position').value=Math.min(Number($('position').max),Number($('position').value)+1);show()};$('play').onclick=()=>{if(timer){clearInterval(timer);timer=null;$('play').textContent='Play'}else{timer=setInterval(()=>{if(Number($('position').value)>=Number($('position').max)){$('play').click();return}$('next').click()},500);$('play').textContent='Pause'}};$('download').onclick=()=>{if(!current)return;const link=document.createElement('a');link.href=URL.createObjectURL(new Blob([JSON.stringify(current.trace,null,2)],{type:'application/json'}));link.download=`trajectory-${current.trace.seed}.json`;link.click();URL.revokeObjectURL(link.href)};$('downloadExplanation').onclick=()=>{if(!explanation)return;const link=document.createElement('a');link.href=URL.createObjectURL(new Blob([JSON.stringify(explanation,null,2)],{type:'application/json'}));link.download=`explanation-${current.trace.seed}-${explanation.step}.json`;link.click();URL.revokeObjectURL(link.href)};addEventListener('keydown',event=>{if(event.key==='ArrowLeft')$('previous').click();if(event.key==='ArrowRight')$('next').click()});catalog().catch(error=>setStatus(error.message,true));
</script>'''
