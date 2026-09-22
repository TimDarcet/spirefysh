import ctypes
import os
import signal
import threading
import time
import traceback
from queue import Empty, Full

import numpy as np
import torch

import sts2_sim
from model import *
from telemetry import *





class WinningReservoir:
    def __init__(self, capacity, envs, pending_capacity=None):
        self.capacity = capacity
        self.pending_capacity = pending_capacity or max(1, capacity // envs)
        self.pending = [[] for _ in range(envs)]
        self.pending_seen = [0] * envs
        self.rows = []
        self.completed = []
        self.character_seen = [0] * 5
        self.seen = self.wins = self.skipped = self.forced = 0

    def record(self, rows, choices, log_probabilities, policies, values, rng):
        if not self.capacity:
            return
        policies = policies if policies is not None else [None] * len(rows)
        for index, (row, choice, log_probability, policy, value) in enumerate(
            zip(rows, choices, log_probabilities, policies, values)
        ):
            if packed_legal_count(row) <= 1:
                self.forced += 1
                continue
            episode = self.pending[index]
            self.pending_seen[index] += 1
            item = (row, int(choice), float(log_probability), 1.0 - float(value),
                    None if policy is None else
                    np.asarray(policy[:packed_action_count(row)], np.float16))
            if len(episode) < self.pending_capacity:
                episode.append(item)
            else:
                slot = int(rng.integers(self.pending_seen[index]))
                if slot < self.pending_capacity:
                    episode[slot] = item

    def finish(self, indices, won, rng):
        if not self.capacity:
            return
        for index, victory in zip(indices, won):
            episode = self.pending[index]
            if victory:
                self.wins += 1
                for row, action, probability, advantage, policy in episode:
                    if advantage <= 0:
                        self.skipped += 1
                        continue
                    self.completed.append(
                        (row, action, probability, min(advantage, 1.0), policy)
                    )
            episode.clear()
            self.pending_seen[index] = 0

    def admit(self, rows, wins, skipped, forced, rng):
        self.wins += wins
        self.skipped += skipped
        self.forced += forced
        groups = self.by_character()
        added = 0
        for item in rows:
            character = packed_character(item[0])
            quota = self.capacity // 5 + (character < self.capacity % 5)
            self.seen += 1
            self.character_seen[character] += 1
            if len(groups[character]) < quota:
                groups[character].append(len(self.rows))
                self.rows.append(item)
                added += 1
                continue
            slot = int(rng.integers(self.character_seen[character]))
            if slot < quota:
                self.rows[groups[character][slot]] = item
                added += 1
        return added

    def evict(self, rows):
        rejected = {id(row) for row in rows}
        before = len(self.rows)
        self.rows = [row for row in self.rows if id(row) not in rejected]
        return before - len(self.rows)

    def discard(self, indices):
        for index in indices:
            self.pending[index].clear()
            self.pending_seen[index] = 0

    def clear(self):
        self.pending = [[] for _ in self.pending]
        self.pending_seen = [0] * len(self.pending_seen)
        self.rows.clear()
        self.completed.clear()
        self.character_seen = [0] * 5
        self.seen = self.wins = self.skipped = self.forced = 0

    def state_dict(self):
        return {
            "rows": list(self.rows), "seen": self.seen, "wins": self.wins,
            "skipped": self.skipped, "forced": self.forced,
            "character_seen": self.character_seen,
        }

    def load_state_dict(self, state):
        rows = [row if len(row) == 5 else (*row, None) for row in state.get("rows", ())]
        groups = [[] for _ in range(5)]
        for row in rows:
            character = packed_character(row[0])
            if len(groups[character]) < self.capacity // 5 + (character < self.capacity % 5):
                groups[character].append(row)
        self.rows = [row for group in groups for row in group]
        self.completed.clear()
        self.seen = max(len(self.rows), state.get("seen", 0))
        self.wins = state.get("wins", 0)
        self.skipped = state.get("skipped", 0)
        self.forced = state.get("forced", 0)
        seen = list(state.get("character_seen", map(len, groups)))
        self.character_seen = [max(len(group), seen[index]) for index, group in enumerate(groups)]

    def by_character(self):
        groups = [[] for _ in range(5)]
        for index, (row, *_rest) in enumerate(self.rows):
            character = packed_character(row)
            groups[character].append(index)
        return groups

    def drain_candidates(self):
        result = {
            "rows": self.completed, "wins": self.wins,
            "skipped": self.skipped, "forced": self.forced,
        }
        self.completed = []
        self.wins = self.skipped = self.forced = 0
        return result

    def sample(self, count, rng):
        pool = [index for index, row in enumerate(self.rows) if row[4] is not None]
        selected = rng.choice(pool, min(count, len(pool)), replace=False)
        return [self.rows[index] for index in selected]


def act(model, observation, target, sample, precision, generator=None, temperature=1,
        trace_indices=()):
    with torch.inference_mode():
        inputs = tensors(observation, torch.device("cpu") if target.type == "mps" else target, model)
        if target.type == "mps":
            inputs = upload(inputs, target)
        logits, critic_logits = predict(
            model, inputs, precision, temperature, flat_policy=True,
        )
        action_row = inputs[5][0][:len(logits)]
        action_flat = inputs[5][1][:len(logits)]
        legal = inputs[5][2][:len(logits)]
        masked = logits.masked_fill(~legal, -torch.inf)
        if sample:
            race = torch.empty_like(masked).exponential_(generator=generator).log() - masked
            minimum = race.new_full((len(observation[0]),), torch.inf).scatter_reduce_(
                0, action_row, race, reduce="amin",
            )
            selected = (race == minimum[action_row]).nonzero().squeeze(1)
        else:
            maximum = masked.new_full((len(observation[0]),), -torch.inf).scatter_reduce_(
                0, action_row, masked, reduce="amax",
            )
            selected = (masked == maximum[action_row]).nonzero().squeeze(1)
        choice = action_flat[selected] % inputs[5][4]
        assert len(choice) == len(observation[0]) and legal[selected].all()
    values = critic_value(critic_logits)
    policy = None
    details = {
        int(index): (
            torch.log_softmax(masked[action_row == index], 0).cpu().tolist(),
            float(values[index]),
        )
        for index in trace_indices
    }
    return (choice.cpu().numpy(), masked[selected].cpu().numpy(),
            policy, values.cpu().numpy(), details)


def cuts(done, episode_steps, combat_steps, still_combat, legal, max_steps, max_combat_steps):
    step = episode_steps >= max_steps
    combat = (combat_steps >= max_combat_steps) & still_combat
    empty = ~legal.any(1)
    cut = (step | combat | empty) & ~done
    return cut, step & cut, combat & cut, empty & cut


def sampling_steps(produced, budget, envs, limit):
    return max(0, min(limit, (budget - produced + envs - 1) // envs))


def bounded_mcts_timeout(requested, watchdog):
    return min(requested or watchdog / 2, watchdog / 2)


def sampler_wedged(heartbeat, observed_stale, now, timeout):
    return (now - heartbeat > timeout and observed_stale is not None
            and now - observed_stale > timeout / 10)


class RolloutCollector:
    def __init__(self, args, sampler_session, stage, reservoir, iteration=0, worker=0,
                 generation=0):
        ascension, bonus = STAGES[stage]
        self.args = args
        seed_worker = worker + generation * args.samplers
        self.env = sts2_sim.Batch(
            args.envs, args.training_seed + sampler_session * 10_000_000 + seed_worker * args.envs,
            None,
            ascension=ascension,
        )
        self.env.set_training_bonus(bonus)
        self.env.set_potential_weights([
            getattr(args, f"potential_{term}_weight") for term in POTENTIAL_TERMS
        ])
        self.observation = self.env.observe_tokens(flat=True)
        self.episode_steps = np.zeros(args.envs, np.int32)
        self.combat_steps = np.zeros(args.envs, np.int32)
        self.reservoir = reservoir
        seed = args.seed + sampler_session * 10_000_000 + seed_worker
        self.rng = np.random.default_rng(seed)
        self.torch_rng = torch.Generator().manual_seed(seed)
        self.iteration, self.sampler_session, self.stage = iteration, sampler_session, stage
        self.worker, self.generation = worker, generation
        self.trajectories = [None] * args.envs
        self.action_history = [[] for _ in range(args.envs)]
        self.traces = [None] * args.envs
        self.native_steps = []
        self.native_starts = np.zeros(args.envs, np.int64)
        self.native_started = np.full(args.envs, time.monotonic())

    def event(self, event, **values):
        emit_event({"event": event, **values}, role=f"sampler-{self.worker}")

    def trace_indices(self):
        if not getattr(self.args, "capture_trajectories", False):
            return []
        characters, seeds = self.env.characters(), self.env.seeds()
        active = {trace["character"] for trace in self.traces if trace is not None}
        for character in range(5):
            if character in active:
                continue
            index = next((index for index, current in enumerate(characters)
                          if current == character and not self.episode_steps[index]
                          and self.traces[index] is None), None)
            if index is not None:
                self.traces[index] = {
                    "trajectory_schema": 1,
                    "id": (f"{self.sampler_session}:{self.worker}:{self.generation}:"
                           f"{self.iteration}:{seeds[index]}"),
                    "sampler_session": self.sampler_session,
                    "worker": self.worker, "generation": self.generation,
                    "started_iteration": self.iteration,
                    "seed": int(seeds[index]), "character": character,
                    "stage": self.stage, "ascension": STAGES[self.stage][0],
                    "bonus": STAGES[self.stage][1],
                    "temperature": self.args.policy_temperature, "sampled": True,
                    "fingerprint": self.env.fingerprint(),
                    "choices": [], "log_policies": [], "critic_values": [],
                    "canonical_progress": [], "phases": [], "policy_revisions": [],
                    "started": time.monotonic(),
                }
        return [index for index, trace in enumerate(self.traces) if trace is not None]

    def finish_trace(self, index, outcome, terminal_floor, terminal):
        trace = self.traces[index]
        if trace is None:
            return
        trace["completion_seconds"] = time.monotonic() - trace.pop("started")
        trace["terminal_floor"] = terminal_floor
        trace["terminal"] = bool(terminal)
        trace["outcome"] = outcome
        trace["completed_iteration"] = self.iteration
        self.event("trajectory", **trace)
        self.traces[index] = None

    def trace_empty(self, kind, indices, characters, stats):
        seeds = self.env.seeds()
        for index in indices:
            emit_event({
                "event": "empty_actions",
                "kind": kind, "sampler_session": self.sampler_session, "worker": self.worker,
                "iteration": self.iteration, "env": index, "seed": seeds[index],
                "character": int(characters[index]), "stage": self.stage,
                "stats": list(stats[index]), "actions": self.action_history[index],
            }, "WARNING", role=f"sampler-{self.worker}")

    def collect(self, model, target, precision, deadline, steps, version,
                stop=None, heartbeat=None, progress=None, sampling=None, paused=None):
        args = self.args
        native = model is None
        cache_start = dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0) \
            if native else model.cache_stats.copy()
        finished = []
        completions = []
        episodes = [[] for _ in range(5)]
        sample_keys = (
            "rows", "choices", "old_log", "critic_values", "canonical_progress",
            "phases", "win_rewards", "terminals", "characters", "versions", "potentials",
        ) + (("features",) if native and args.cache_features else ())
        def materialize(trajectory):
            samples = trajectory.pop("samples")
            trajectory.update(zip(sample_keys, map(list, zip(*samples))))
            return trajectory
        discarded_steps = orphan_empty_actions = sampled_steps = 0
        search_stats = np.zeros(17, np.int64)
        expert_rows = []
        collect_seconds = 0.0
        for _ in range(steps):
            while sampling is not None and not sampling.is_set():
                paused[self.worker] = True
                if heartbeat is not None:
                    heartbeat[self.worker] = time.monotonic()
                if time.monotonic() >= deadline or stop.wait(.01):
                    break
            if paused is not None:
                paused[self.worker] = False
            if time.monotonic() >= deadline or stop and stop.is_set():
                break
            if heartbeat is not None:
                heartbeat[self.worker] = time.monotonic()
            self.iteration += 1
            empty = np.zeros(args.envs, bool) if native else ~observation_legal(self.observation).any(1)
            if empty.any():
                reset = np.flatnonzero(empty).tolist()
                stale_characters = np.asarray(self.observation[0], np.uint8)
                stale_stats = self.env.stats()
                self.trace_empty("orphan", reset, stale_characters, stale_stats)
                orphan_empty_actions += len(reset)
                self.reservoir.discard(reset)
                for index in reset:
                    stats = stale_stats[index]
                    self.finish_trace(
                        index, "empty_actions", int((stats[0] - 1) * 17 + stats[1]), False,
                    )
                    if self.trajectories[index] is not None:
                        discarded_steps += len(self.trajectories[index]["samples"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                self.observation = self.env.observe_tokens(flat=True)
            trace_indices = self.trace_indices()
            step_started = time.monotonic()
            if native:
                trace_details = ({
                    index: (policy, value)
                    for index, policy, value in self.env.policy_details(
                        trace_indices, args.policy_temperature,
                    )
                } if trace_indices else {})
                result = self.env.policy(
                    args.policy_temperature,
                    advance=True,
                    mcts_fraction=args.mcts_fraction,
                    mcts_simulations=args.mcts_simulations,
                    mcts_boss_simulations=args.mcts_boss_simulations,
                    mcts_turns=args.mcts_turns,
                    mcts_max_depth=args.mcts_max_depth,
                    mcts_batch_size=args.mcts_batch_size,
                    mcts_min_visits=args.mcts_min_visits,
                    mcts_max_targets=args.mcts_max_targets,
                    mcts_prior_temperature=args.mcts_prior_temperature,
                    mcts_q_temperature=args.mcts_q_temperature,
                    mcts_exploration=args.mcts_exploration,
                    mcts_value_consistency=getattr(args, "search_consistency_weight", 0) > 0,
                    mcts_heuristic=args.mcts_heuristic,
                    mcts_timeout=bounded_mcts_timeout(args.mcts_timeout, args.sampler_timeout),
                    cache_features=args.cache_features,
                    skip_forced=True,
                )
                characters, choice, log_probability, critic_value, step_rows, \
                    step_experts, step_search_stats, *cached = result
                raw_reward, done, stats, next_legal, in_combat, canonical, phases, potentials, \
                    *cached = cached
                step_features = cached[0] if cached else None
                if heartbeat is not None:
                    heartbeat[self.worker] = time.monotonic()
                characters = np.asarray(characters, np.uint8)
                critic_value = np.asarray(critic_value, np.float32)
                potentials = np.asarray(potentials, np.float32)
                search_stats += np.asarray(step_search_stats, np.int64)
                for expert in step_experts:
                    row, target, _visits, _depth, *consistency = expert
                    consistency = (((), np.empty(0, np.float32), 1., 0.)
                                   if not consistency else
                                   (tuple(consistency[0]), np.asarray(consistency[1], np.float32),
                                    consistency[2], consistency[3]))
                    expert_rows.append((row, np.asarray(target, np.float16), version,
                                        consistency))
                policy = None
            else:
                state_stats = self.env.stats()
                canonical = np.asarray([row[9] for row in state_stats], np.uint8)
                phases = np.asarray([row[4] for row in state_stats], np.uint8)
                characters = np.asarray(self.observation[0], np.uint8)
                potentials = np.asarray(self.observation[4], np.float32)
                choice, log_probability, policy, critic_value, trace_details = act(
                    model, self.observation, target, True, precision, self.torch_rng,
                    args.policy_temperature, trace_indices,
                )
                step_rows = _pack_batch(self.observation)
            if not native:
                for index, action in enumerate(choice):
                    self.action_history[index].append(int(action))
            for index in trace_indices:
                trace_policy, trace_value = trace_details[index]
                self.traces[index]["choices"].append(int(choice[index]))
                self.traces[index]["log_policies"].append([
                    float(value) for value in trace_policy
                ])
                self.traces[index]["critic_values"].append(float(trace_value))
                self.traces[index]["canonical_progress"].append(int(canonical[index]))
                self.traces[index]["phases"].append(int(phases[index]))
                self.traces[index]["policy_revisions"].append(int(version))
            self.reservoir.record(
                step_rows, choice, log_probability, policy,
                critic_value + potentials, self.rng,
            )
            in_combat = np.asarray(in_combat, bool) if native else np.asarray([
                row[4] == 1 for row in self.env.stats()
            ])
            self.combat_steps = np.where(in_combat, self.combat_steps + 1, 0)
            if not native:
                raw_reward, done, _ = self.env.step(choice.tolist())
            raw_reward = np.array(raw_reward, np.float32, copy=True)
            done = np.asarray(done, bool)
            sampled_steps += args.envs
            if progress is not None:
                progress[self.worker] += args.envs
            self.episode_steps += 1
            next_observation = None if native else self.env.observe_tokens((~done).tolist(), True)
            stats = stats if native else self.env.stats()
            still_combat = np.asarray([row[4] == 1 for row in stats])
            truncated, step_truncated, combat_truncated, empty_actions = cuts(
                done, self.episode_steps, self.combat_steps, still_combat,
                np.asarray(next_legal)[:, None]
                if native else observation_legal(next_observation),
                args.max_steps, args.max_combat_steps,
            )
            boundary = done | truncated
            if empty_actions.any():
                reset = np.flatnonzero(empty_actions).tolist()
                self.trace_empty("post_step", reset, characters, stats)
            if native:
                self.native_steps.append((
                    step_rows, choice, log_probability, critic_value, canonical, phases,
                    raw_reward, done, characters, version, potentials,
                ) + ((step_features,) if args.cache_features else ()))
            else:
                for index, row in enumerate(step_rows):
                    trajectory = self.trajectories[index]
                    if trajectory is None:
                        trajectory = {"samples": [], "started": step_started}
                        self.trajectories[index] = trajectory
                    trajectory["samples"].append((
                        row, choice[index], log_probability[index], critic_value[index],
                        canonical[index], phases[index], raw_reward[index], done[index],
                        characters[index], version, potentials[index],
                    ))
            if boundary.any():
                reset = np.flatnonzero(boundary).tolist()
                wins = [bool(done[index] and stats[index][4] == 12) for index in reset]
                self.reservoir.finish(reset, wins, self.rng)
                for index, won in zip(reset, wins):
                    history = (self.native_steps[self.native_starts[index]:] if native
                               else self.trajectories[index]["samples"])
                    terminal_floor = int((stats[index][0] - 1) * 17 + stats[index][1])
                    completion_seconds = (time.monotonic() - self.native_started[index] if native
                                          else time.monotonic() - self.trajectories[index]["started"])
                    policy_versions = [row[9] for row in history]
                    completions.append({
                        "character": int(characters[index]),
                        "floor": terminal_floor,
                        "terminal": bool(done[index]), "won": won,
                        "step_cap": bool(step_truncated[index] and truncated[index]),
                        "combat_cap": bool(combat_truncated[index] and truncated[index]),
                        "empty_actions": bool(empty_actions[index]),
                        "length": len(history), "completion_seconds": completion_seconds,
                        "iteration": self.iteration,
                        "policy_revision_min": min(policy_versions, default=version),
                        "policy_revision_max": max(policy_versions, default=version),
                    })
                    episodes[characters[index]].append((
                        int(done[index] and stats[index][4] == 12),
                        (stats[index][0] - 1) * 17 + stats[index][1],
                        int(step_truncated[index] and truncated[index]),
                        int(combat_truncated[index] and truncated[index]),
                        int(empty_actions[index]),
                        self.iteration,
                    ))
                    outcome = (
                        "won" if won else "dead" if done[index]
                        else "empty_actions" if empty_actions[index]
                        else "step_cap" if step_truncated[index]
                        else "combat_cap"
                    )
                    self.finish_trace(index, outcome, terminal_floor, done[index])
                    if native and done[index]:
                        trajectory = {
                            key: ([step[column][index] for step in history]
                                  if column in (0, 11) else np.asarray([
                                      step[column] if column == 9 else step[column][index]
                                      for step in history
                                  ]))
                            for column, key in enumerate(sample_keys)
                        }
                        trajectory["completion_seconds"] = completion_seconds
                        trajectory["terminal_floor"] = terminal_floor
                        finished.append(trajectory)
                    elif native:
                        discarded_steps += len(self.native_steps) - int(self.native_starts[index])
                    elif done[index]:
                        trajectory = self.trajectories[index]
                        trajectory["completion_seconds"] = completion_seconds
                        trajectory["terminal_floor"] = terminal_floor
                        trajectory.pop("started")
                        finished.append(materialize(trajectory))
                    else:
                        discarded_steps += len(self.trajectories[index]["samples"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                    self.native_starts[index] = len(self.native_steps)
                    self.native_started[index] = time.monotonic()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                next_observation = None if native else self.env.observe_tokens(flat=True)
                if native:
                    drop = int(self.native_starts.min())
                    if drop:
                        del self.native_steps[:drop]
                        self.native_starts -= drop
            self.observation = next_observation
            collect_seconds += time.monotonic() - step_started
            if finished and not native:
                break
        return {
            "trajectories": finished,
            "completions": completions,
            "episodes": episodes,
            "iteration": self.iteration,
            "orphan_empty_actions": orphan_empty_actions,
            "collect_seconds": collect_seconds, "discarded_steps": discarded_steps,
            "sampled_steps": sampled_steps,
            "expert_rows": expert_rows,
            "mcts_roots": int(search_stats[0]), "mcts_simulations": int(search_stats[1]),
            "mcts_leaves": int(search_stats[2]), "mcts_nodes": int(search_stats[3]),
            "mcts_batches": int(search_stats[4]), "mcts_targets": int(search_stats[5]),
            "mcts_turn_starts": int(search_stats[6]),
            "mcts_seconds": float(search_stats[7]) / 1e6,
            "mcts_simulate_seconds": float(search_stats[8]) / 1e6,
            "mcts_encode_seconds": float(search_stats[9]) / 1e6,
            "mcts_inference_seconds": float(search_stats[10]) / 1e6,
            "mcts_backup_seconds": float(search_stats[11]) / 1e6,
            "mcts_rollout_steps": int(search_stats[12]),
            "mcts_rollout_completed": int(search_stats[13]),
            "mcts_rollout_invalid": int(search_stats[14]),
            "mcts_rollout_seconds": float(search_stats[15]) / 1e6,
            "mcts_timeouts": int(search_stats[16]),
            **({key: 0 for key in cache_start} if native else {
                key: model.cache_stats[key] - cache_start[key] for key in model.cache_stats
            }),
        }

    def run(self, model, models, samples, stop, deadline, budget, worker=0,
            heartbeat=None, progress=None, version=0, actor_revision=0, target=None,
            precision="fp32", sampling=None, paused=None):
        target = target or torch.device("cpu")
        produced = 0
        def empty():
            return {
                "trajectories": [], "completions": [], "episodes": [[] for _ in range(5)],
                "iteration": self.iteration, "orphan_empty_actions": 0,
                "collect_seconds": 0.0, "queue_full_waits": 0,
                "queue_put_seconds": 0.0,
                "discarded_steps": 0, "sampled_steps": 0,
                "expert_rows": [],
                "mcts_roots": 0, "mcts_simulations": 0, "mcts_leaves": 0,
                "mcts_nodes": 0, "mcts_batches": 0, "mcts_targets": 0,
                "mcts_turn_starts": 0,
                "mcts_seconds": 0.0,
                "mcts_simulate_seconds": 0.0, "mcts_encode_seconds": 0.0,
                "mcts_inference_seconds": 0.0, "mcts_backup_seconds": 0.0,
                "mcts_rollout_steps": 0, "mcts_rollout_completed": 0,
                "mcts_rollout_invalid": 0, "mcts_rollout_seconds": 0.0,
                "mcts_timeouts": 0,
                **dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0),
            }
        pending = empty()
        pending_rows = 0
        while produced < budget and time.monotonic() < deadline and not stop.is_set():
            latest = None
            while True:
                try:
                    latest = models.get_nowait()
                except (Empty, EOFError, OSError):
                    break
            if latest:
                actor_revision, version, state = latest
                if model is None:
                    self.env.load_policy(state)
                else:
                    model.load_state_dict(state)
                if self.args.log_level == "DEBUG":
                    self.event("actor_loaded", actor_revision=actor_revision,
                               policy_revision=version)
            steps = sampling_steps(produced, budget, self.args.envs, self.args.sampler_steps)
            if not steps:
                break
            result = self.collect(
                model, target, precision, deadline, steps, version, stop,
                heartbeat, progress, sampling, paused,
            )
            if self.args.log_level == "DEBUG":
                self.event(
                    "collect", version=version, iteration=result["iteration"],
                    sampled=result["sampled_steps"], trajectories=len(result["trajectories"]),
                    rows=sum(len(row["rows"]) for row in result["trajectories"]),
                    seconds=round(result["collect_seconds"], 3),
                    mcts_roots=result["mcts_roots"], mcts_simulations=result["mcts_simulations"],
                    mcts_targets=result["mcts_targets"], mcts_seconds=round(result["mcts_seconds"], 3),
                )
            pending["trajectories"].extend(result["trajectories"])
            pending["completions"].extend(result["completions"])
            pending["expert_rows"].extend(result["expert_rows"])
            for target_episodes, rows in zip(pending["episodes"], result["episodes"]):
                target_episodes.extend(rows)
            for key in (
                "orphan_empty_actions", "collect_seconds",
                "discarded_steps", "sampled_steps", "card_hit", "card_miss",
                "graph_hit", "graph_miss", "mcts_roots", "mcts_simulations",
                "mcts_leaves", "mcts_nodes", "mcts_batches", "mcts_targets",
                "mcts_turn_starts", "mcts_seconds",
                "mcts_simulate_seconds", "mcts_encode_seconds",
                "mcts_inference_seconds", "mcts_backup_seconds",
                "mcts_rollout_steps", "mcts_rollout_completed", "mcts_rollout_invalid",
                "mcts_rollout_seconds", "mcts_timeouts",
            ):
                pending[key] += result[key]
            pending["iteration"] = result["iteration"]
            added = sum(len(trajectory["rows"]) for trajectory in result["trajectories"])
            pending_rows += added
            produced += added
            if max(pending_rows, len(pending["completions"])) < self.args.envs * 4 \
                    and produced < budget \
                    and time.monotonic() < deadline and not stop.is_set():
                continue
            pending["reservoir"] = self.reservoir.drain_candidates()
            pending["queued_at"] = time.monotonic()
            put_started = pending["queued_at"]
            while not stop.is_set():
                try:
                    pending["queue_put_seconds"] = time.monotonic() - put_started
                    samples.put_nowait((worker, self.generation, version, pending))
                    if self.args.log_level == "DEBUG":
                        self.event(
                            "packet", version=version, iteration=pending["iteration"],
                            trajectories=len(pending["trajectories"]), rows=pending_rows,
                            sampled=pending["sampled_steps"], discarded=pending["discarded_steps"],
                        )
                    pending = empty()
                    pending_rows = 0
                    break
                except Full:
                    pending["queue_full_waits"] += 1
                    if heartbeat is not None:
                        heartbeat[worker] = time.monotonic()
                    time.sleep(.01)
        pending["discarded_steps"] += (
            sum(len(self.native_steps) - int(start) for start in self.native_starts)
            if model is None else
            sum(len(trajectory["samples"]) for trajectory in self.trajectories
                if trajectory is not None)
        ) + sum(len(trajectory["rows"]) for trajectory in pending["trajectories"])
        pending["trajectories"].clear()
        active = [index for index, trajectory in enumerate(self.trajectories) if trajectory is not None]
        self.reservoir.discard(active)
        changes = self.reservoir.drain_candidates()
        if "reservoir" in pending:
            for key in changes:
                changes[key] = pending["reservoir"][key] + changes[key]
        pending["reservoir"] = changes
        pending["iteration"] = self.iteration
        if heartbeat is not None:
            heartbeat[worker] = time.monotonic()
        return worker, self.generation, version, pending


def collect_worker(model, args, sampler_session, stage, capacity, pending_capacity, iteration,
                   worker, generation, actor_revision, policy_revision, models, samples, stop,
                   deadline, budget, results, heartbeat, progress, sampling, paused, parent_pid):
    if args.sampler_backend == "process":
        prctl = getattr(ctypes.CDLL(None), "prctl", None)
        if prctl is not None and prctl(1, signal.SIGKILL) == 0:
            if os.getppid() != parent_pid:
                os._exit(1)
        else:
            def watch_parent():
                while os.getppid() == parent_pid:
                    time.sleep(1)
                os._exit(1)
            threading.Thread(target=watch_parent, name="parent-watchdog", daemon=True).start()
        release_accelerator()
        configure_logging(args.output, f"sampler-{worker}", getattr(args, "log_level", "INFO"),
                          args.trainer_session, args.run_id, args.event_log)
    qos = getattr(ctypes.CDLL(None), "pthread_set_qos_class_self_np", None)
    if qos is not None:
        qos(int(os.environ.get("ACTOR_QOS", "0x21"), 0), 0)
    if args.sampler_backend == "process":
        os.environ["RAYON_NUM_THREADS"] = str(args.sampler_threads)
        torch.set_num_threads(args.torch_threads)
        torch.set_num_interop_threads(1)
        torch._C._set_default_mobile_cpu_allocator()
    reservoir = WinningReservoir(capacity, args.envs, pending_capacity)
    collector = RolloutCollector(
        args, sampler_session, stage, reservoir, iteration, worker, generation,
    )
    collector.event(
        "sampler_start", worker=worker, generation=generation,
        sampler_session=sampler_session, stage=stage, envs=args.envs,
        threads=args.sampler_threads, actor_revision=actor_revision,
        policy_revision=policy_revision,
    )
    target = torch.device("mps") if os.environ.get("ACTOR_MPS") else torch.device("cpu")
    precision = "bf16" if target.type == "mps" else "fp32"
    if target.type == "mps":
        model = model.to(target)
        collector.torch_rng = torch.Generator(device=target).manual_seed(
            args.seed + sampler_session * 10_000_000 + worker + generation * args.samplers
        )
    if isinstance(model, bytes):
        collector.env.load_policy(model)
        collector.observation = None
        model = None
    heartbeat[worker] = time.monotonic()
    try:
        result = collector.run(
            model, models, samples, stop, deadline, budget, worker, heartbeat, progress,
            policy_revision, actor_revision, target, precision, sampling, paused,
        )
        collector.event(
            "sampler_stop", policy_revision=result[2], iteration=result[3]["iteration"],
            sampled=result[3]["sampled_steps"], discarded=result[3]["discarded_steps"],
        )
        results.put(result)
    except BaseException as error:
        collector.event("sampler_error", policy_revision=policy_revision,
                        error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        if args.sampler_backend == "process":
            shutdown_logging()
