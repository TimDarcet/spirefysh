import copy
import ctypes
import fcntl
import json
import math
import multiprocessing
import os
import struct
import tempfile
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from itertools import islice
from pathlib import Path
from queue import Empty, Full, Queue

import numpy as np
import torch
from torch import nn

import sts2_sim
from model import *
from rollout import *
from telemetry import *





def upload(value, target):
    marker = object()
    tensors = []

    def flatten(item):
        if torch.is_tensor(item):
            tensors.append(item)
            return marker, len(tensors) - 1
        if isinstance(item, tuple):
            return tuple(flatten(child) for child in item)
        return item

    structure = flatten(value)
    uploaded = [None] * len(tensors)
    for dtype in {tensor.dtype for tensor in tensors}:
        selected = [index for index, tensor in enumerate(tensors) if tensor.dtype == dtype]
        sizes = [tensors[index].numel() for index in selected]
        buffer = torch.cat([tensors[index].reshape(-1) for index in selected]).to(
            target, non_blocking=True,
        )
        offset = 0
        for index, size in zip(selected, sizes):
            uploaded[index] = buffer[offset:offset + size].view(tensors[index].shape)
            offset += size

    def rebuild(item):
        if isinstance(item, tuple) and len(item) == 2 and item[0] is marker:
            return uploaded[item[1]]
        if isinstance(item, tuple):
            return tuple(rebuild(child) for child in item)
        return item

    return rebuild(structure)


def trust_region_step(model, optimizer, inputs, action, old, fresh, denominator, precision, limit,
                      temperature=1, attempts=3, choice_index=None):
    parameters = [parameter for parameter in model.parameters() if parameter.grad is not None]
    gradient_norm = float(nn.utils.clip_grad_norm_(parameters, .5))
    optimizer_state = copy.deepcopy(optimizer.state_dict()) if any(
        parameter.grad is not None and parameter not in optimizer.state for parameter in parameters
    ) else None
    cache = getattr(optimizer, "_trust_region_saved", {})
    key = tuple(map(id, parameters))
    saved = cache.get(key)
    if saved is None:
        saved = [value.detach().clone() for value in parameters]
        cache[key] = saved
        optimizer._trust_region_saved = cache
    else:
        torch._foreach_copy_(saved, parameters)

    def restore():
        with torch.no_grad():
            torch._foreach_copy_(parameters, saved)
        if optimizer_state is not None:
            optimizer.load_state_dict(optimizer_state)
        else:
            with torch.no_grad():
                for parameter in parameters:
                    state = optimizer.state[parameter]
                    beta1, beta2 = optimizer.param_groups[0]["betas"]
                    state["exp_avg"].sub_(parameter.grad, alpha=1 - beta1).div_(beta1)
                    state["exp_avg_sq"].addcmul_(
                        parameter.grad, parameter.grad, value=beta2 - 1,
                    ).div_(beta2)
                    state["step"].sub_(1)

    proposals = []
    try:
        optimizer.step()
        for attempt in range(attempts):
            if attempt:
                with torch.no_grad():
                    for parameter, original in zip(parameters, saved):
                        parameter.sub_(original).mul_(.25).add_(original)
            with torch.no_grad():
                kind = inputs[0].device.type
                dtype = torch.bfloat16 if precision == "bf16" else torch.float16
                with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
                    logits = model(
                        *inputs[:6], policy_only=True, flat_policy=choice_index is not None,
                        temperature=temperature,
                    ).float()
                selected = logits[choice_index] if choice_index is not None else \
                    logits[:len(action)].gather(1, action[:, None]).squeeze(1)
                log_ratio = selected - old
                post_kl = (((log_ratio.exp() - 1 - log_ratio) * fresh).sum() / denominator).float()
            proposals.append(float(post_kl))
            if torch.isfinite(post_kl) and proposals[-1] <= limit:
                return True, proposals, log_ratio, gradient_norm
        restore()
        return False, proposals, None, gradient_norm
    except Exception:
        restore()
        raise


def critic_only_step(model, optimizer, parameters=None):
    parameters = parameters or tuple(model.critic.parameters())
    selected = {id(parameter) for parameter in parameters}
    for parameter in model.parameters():
        if id(parameter) not in selected:
            parameter.grad = None
    gradient_norm = float(nn.utils.clip_grad_norm_(parameters, .5))
    optimizer.step()
    return gradient_norm


def set_learning_rate(optimizer, rate, warmup_steps, weights_revision):
    warmup = min(1., (weights_revision + 1) / warmup_steps) if warmup_steps else 1.
    for group in optimizer.param_groups:
        group["lr"] = rate * group["lr_scale"] * warmup


def promotion_sample(episodes, size, threshold):
    recent = episodes[-size:]
    grouped = [[row for character, row in recent if character == value] for value in range(5)]
    result = episode_summary([row for _, row in recent]) | {
        "runs": len(recent),
        "characters": [episode_summary(rows) | {
            "character": character, "runs": len(rows),
        } for character, rows in enumerate(grouped)],
    }
    return len(recent) == size and all(
        rows and sum(row[0] for row in rows) / len(rows) > threshold for rows in grouped
    ), result


def curriculum_weights(stage, stage_decisions, auxiliary_decisions, args, active=True):
    stage_fraction = min(1, stage_decisions / args.progress_decisions)
    progress_fraction = min(1, auxiliary_decisions / args.progress_decisions)
    scale = max(.35, .8 ** stage)
    fixed_entropy = getattr(args, "entropy_weight", None)
    beta = 0 if not active else (
        args.progress_beta if args.progress_beta is not None
        else 1 - .9 * progress_fraction
    )
    entropy_start, entropy_end = (
        (fixed_entropy, fixed_entropy) if fixed_entropy is not None
        else (args.entropy_start * scale, args.entropy_end * scale)
    )
    return beta, entropy_start + (entropy_end - entropy_start) * stage_fraction, {
        "stage_decisions": stage_decisions, "auxiliary_decisions": auxiliary_decisions,
        "fraction": stage_fraction, "progress_fraction": progress_fraction,
        "progress_beta_start": 1 if active else 0,
        "progress_beta_end": .1 if active else 0,
        "entropy_start": entropy_start, "entropy_end": entropy_end,
    }


def resume_curriculum(source, stage):
    if not source:
        return 0, stage <= 6
    auxiliary = source.get("auxiliary_decisions", source["decisions"])
    if not stage:
        stage_decisions = auxiliary
    else:
        if "stage_decisions" not in source:
            raise ValueError("checkpoint lacks the current-stage decision clock")
        stage_decisions = source["stage_decisions"]
        if not 0 <= stage_decisions <= auxiliary:
            raise ValueError("invalid current-stage decision clock")
    return stage_decisions, source.get("progress_active", stage <= 6)


class ExperienceDataset:
    def __init__(self):
        self.rows = []
        self.features = []
        self.data = {
            "action": np.empty(0, np.int64), "old": np.empty(0, np.float32),
            "advantage": np.empty(0, np.float32),
            "critic_target": np.empty(0, np.float32),
            "value": np.empty(0, np.float32), "canonical": np.empty(0, np.int8),
            "terminal": np.empty(0, np.int8), "potential": np.empty(0, np.float32),
            "phase": np.empty(0, np.int8), "character": np.empty(0, np.int8),
            "priority": np.empty(0, np.float32),
            "version": np.empty(0, np.int64), "id": np.empty(0, np.int64),
            "trajectory": np.empty(0, np.int64),
        }
        self.next_id = self.next_trajectory = 0
        self.index = {}
        self.capacity = 0
        self.seen = self.admitted = self.uses = self.retired = self.forced_dropped = 0
        self.capacity_dropped = self.incomplete_dropped = 0
        self.stale_dropped = self.ratio_dropped = self.kl_dropped = self.post_kl_dropped = 0

    def __len__(self):
        return len(self.rows)

    def add(self, result, args, limit=None):
        source = result["trajectories"]
        if not source:
            return 0, 0, 0
        fields = (
            "rows", "choices", "old_log", "critic_values", "canonical_progress",
            "phases", "win_rewards", "terminals", "characters", "versions", "potentials",
        )
        trajectories = []
        positions = []
        total = 0
        cached = any("features" in trajectory for trajectory in source)
        for trajectory_id, trajectory in enumerate(source, self.next_trajectory):
            length = len(trajectory["rows"])
            if not length or any(len(trajectory[key]) != length for key in fields):
                raise ValueError("invalid trajectory fields")
            terminal = np.asarray(trajectory["terminals"], bool)
            predicted = np.asarray(trajectory["critic_values"], np.float32)
            canonical = np.asarray(trajectory["canonical_progress"], np.int64)
            potential = np.asarray(trajectory["potentials"], np.float32)
            if (terminal[:-1].any() or not terminal[-1]
                    or predicted.shape != (length,) or not np.isfinite(predicted).all()
                    or canonical.min() < 0 or canonical.max() > MAX_PROGRESS
                    or not np.isfinite(potential).all()):
                raise ValueError("invalid trajectory terminal")
            if cached and len(trajectory.get("features", ())) != length:
                raise ValueError("invalid cached features")
            indices = np.flatnonzero([
                packed_legal_count(row) > 1 for row in trajectory["rows"]
            ])
            if len(indices):
                category = CATEGORIES - 1 if trajectory["win_rewards"][-1] > .5 \
                    else int(canonical.max())
                trajectories.append((trajectory, indices, category, trajectory_id))
                positions.extend(total + indices)
            total += length
        self.next_trajectory += len(source)
        accepted = total if limit is None else max(0, min(total, limit))
        if not positions:
            self.next_id += accepted
            self.seen += accepted
            self.forced_dropped += accepted
            return accepted, accepted, total - accepted
        positions = np.asarray(positions, np.int64)
        selected = positions < accepted
        def joined(key, dtype):
            return np.concatenate([
                np.asarray(trajectory[key], dtype)[indices]
                for trajectory, indices, _category, _trajectory_id in trajectories
            ])
        rows = [trajectory["rows"][index]
                for trajectory, indices, _category, _trajectory_id in trajectories
                for index in indices]
        features = ([trajectory["features"][index]
                     for trajectory, indices, _category, _trajectory_id in trajectories
                     for index in indices]
                    if cached else [])
        advantage = np.empty(len(rows), np.float32)
        targets = np.empty(len(rows), np.float32)
        terminal_categories = np.empty(len(rows), np.int8)
        wins = np.empty(len(rows), bool)
        values = np.empty(len(rows), np.float32)
        terminal = np.zeros(len(rows), bool)
        end = 0
        for trajectory, indices, category, _trajectory_id in trajectories:
            length = len(indices)
            start, end = end, end + length
            predicted = np.asarray(trajectory["critic_values"], np.float32)[indices]
            canonical = np.asarray(trajectory["canonical_progress"], np.int64)[indices]
            potential = np.asarray(trajectory["potentials"], np.float32)[indices]
            terminal_value = category / (CATEGORIES - 1)
            terminal_categories[start:end] = category
            wins[start:end] = category == CATEGORIES - 1
            values[start:end] = predicted
            terminal[end - 1] = True
            reward = args.gae_gamma * np.append(potential[1:], 0.) - potential
            reward[-1] += terminal_value
            gae = next_value = discounted_return = 0.
            for index in range(length - 1, -1, -1):
                delta = reward[index] + args.gae_gamma * next_value - predicted[index]
                gae = delta + args.gae_gamma * args.gae_lambda * gae
                advantage[start + index] = gae
                discounted_return = reward[index] + args.gae_gamma * discounted_return
                targets[start + index] = discounted_return
                next_value = predicted[index]
        priority = 1 + np.abs(advantage) + 4 * terminal + 4 * wins
        data = {
            "action": joined("choices", np.int64),
            "old": joined("old_log", np.float32),
            "advantage": advantage, "critic_target": targets, "value": values,
            "canonical": joined("canonical_progress", np.int8),
            "terminal": terminal_categories, "potential": joined("potentials", np.float32),
            "phase": joined("phases", np.int8),
            "character": joined("characters", np.int8),
            "priority": priority.astype(np.float32),
            "version": joined("versions", np.int64),
            "id": self.next_id + positions,
            "trajectory": np.concatenate([
                np.full(len(indices), trajectory_id, np.int64)
                for _trajectory, indices, _category, trajectory_id in trajectories
            ]),
        }
        size = len(self)
        required = size + int(selected.sum())
        if required > self.capacity:
            self.capacity = max(required, max(1024, self.capacity * 2))
            for key, value in self.data.items():
                storage = np.empty((self.capacity, *value.shape[1:]), value.dtype)
                storage[:size] = value[:size]
                self.data[key] = storage
        self.next_id += accepted
        self.rows.extend(row for row, keep in zip(rows, selected) if keep)
        if features:
            self.features.extend(row for row, keep in zip(features, selected) if keep)
        for key, value in data.items():
            self.data[key][size:required] = value[selected]
        for index, row_id in enumerate(data["id"][selected], size):
            self.index[int(row_id)] = index
        self.seen += accepted
        self.admitted += required - size
        forced = accepted - required + size
        self.forced_dropped += forced
        return accepted, forced, total - accepted

    def trim(self, capacity):
        excess = max(0, len(self) - capacity)
        if excess:
            self.discard_ids(islice(self.index, excess))
            self.capacity_dropped += excess
        return excess

    def discard(self, indices):
        if not len(indices):
            return
        indices = np.unique(indices)
        removed = self.data["id"][indices].copy()
        end = len(self) - len(indices)
        holes = indices[indices < end]
        sources = np.setdiff1d(np.arange(end, len(self)), indices, assume_unique=True)
        for row_id in removed:
            self.index.pop(int(row_id))
        for target, source in zip(holes, sources):
            self.rows[target] = self.rows[source]
            self.index[int(self.data["id"][source])] = int(target)
        del self.rows[end:]
        if self.features:
            for target, source in zip(holes, sources):
                self.features[target] = self.features[source]
            del self.features[end:]
        for key, values in self.data.items():
            values[holes] = values[sources]

    def discard_ids(self, ids):
        indices = np.fromiter(
            (self.index[int(row_id)] for row_id in ids if int(row_id) in self.index),
            np.int64,
        )
        self.discard(indices)
        return len(indices)

    def discard_trajectories(self, ids):
        indices = np.flatnonzero(np.isin(self.data["trajectory"][:len(self)], ids))
        self.discard(indices)
        return len(indices)

    def sample(self, size, rng, balanced=False, candidates=None):
        candidates = np.arange(len(self)) if candidates is None else np.asarray(candidates)
        size = min(size, len(candidates))
        if balanced:
            pools = [list(rng.permutation(np.flatnonzero(
                self.data["character"][candidates] == character)))
                     for character in range(5)]
            pools = [[int(candidates[index]) for index in pool] for pool in pools]
            selected = []
            while len(selected) < size and any(pools):
                for pool in pools:
                    if pool and len(selected) < size:
                        selected.append(pool.pop())
            return np.asarray(selected, np.int64)
        return rng.choice(candidates, size, replace=False)

    def use(self, indices, decay=3):
        self.uses += len(indices)
        self.data["priority"][indices] -= decay
        expired = indices[self.data["priority"][indices] < 0]
        self.retired += len(expired)
        return expired


class CriticBalance:
    def __init__(self, decay=.99, state=None):
        self.decay = decay
        self.frequencies = [np.zeros(size, np.float64) for size in (5, 14, MAX_PROGRESS + 1)]
        self.initialized = False
        if state:
            self.initialized = bool(state.get("initialized"))
            self.frequencies = [np.asarray(row, np.float64) for row in state["frequencies"]]
        self.reset_report()

    def state_dict(self):
        return {"initialized": self.initialized,
                "frequencies": [row.tolist() for row in self.frequencies]}

    def weights(self, character, phase, floor):
        columns = tuple(np.asarray(column, np.int64) for column in (character, phase, floor))
        batch = [np.bincount(column, minlength=len(frequency)) / len(column)
                 for column, frequency in zip(columns, self.frequencies)]
        if self.initialized:
            for frequency, current in zip(self.frequencies, batch):
                frequency *= self.decay; frequency += (1 - self.decay) * current
        else:
            for frequency, current in zip(self.frequencies, batch):
                frequency[:] = current
            self.initialized = True
        weight = np.prod([
            np.maximum(frequency[column], 1e-8) ** -.5
            for frequency, column in zip(self.frequencies, columns)
        ], axis=0) ** (1 / 3)
        weight = np.clip(weight, .25, 4); weight /= weight.mean()
        for count, column in zip(self.counts, columns):
            count += np.bincount(column, minlength=len(count))
        self.ess += weight.sum() ** 2 / np.square(weight).sum()
        self.rows += len(weight); self.batches += 1
        return weight.astype(np.float32)

    def record_loss(self, target, weighted_loss):
        self.target_counts += np.bincount(target, minlength=CATEGORIES)
        np.add.at(self.loss_mass, target, weighted_loss)

    def reset_report(self):
        self.counts = [np.zeros(size, np.int64) for size in (5, 14, MAX_PROGRESS + 1)]
        self.target_counts = np.zeros(CATEGORIES, np.float64)
        self.loss_mass = np.zeros(CATEGORIES, np.float64)
        self.ess = 0.; self.rows = self.batches = 0

    def report(self):
        result = {
            "critic_preweight_character_counts": self.counts[0].tolist(),
            "critic_preweight_phase_counts": self.counts[1].tolist(),
            "critic_preweight_floor_counts": self.counts[2].tolist(),
            "critic_preweight_target_counts": self.target_counts.tolist(),
            "critic_postweight_loss_mass": self.loss_mass.tolist(),
            "critic_weight_ess": self.ess / max(1, self.batches),
            "critic_weight_ess_fraction": self.ess / max(1, self.rows),
        }
        self.reset_report()
        return result

class ExpertDataset:
    def __init__(self, capacity):
        self.capacity = capacity
        self.rows = []
        self.targets = []
        self.consistencies = []
        self.versions = np.empty(0, np.int64)
        self.visits = np.empty(0, np.int32)
        self.depths = np.empty(0, np.int32)
        self.ids = np.empty(0, np.int64)
        self.next_id = 0
        self.seen = self.used = self.stale = self.evicted = 0

    def __len__(self):
        return len(self.rows)

    def add(self, rows):
        if not rows:
            return
        for row, target, _version, _visits, _depth, consistency in rows:
            target = np.asarray(target, np.float16)
            if (target.shape != (packed_action_count(row),) or not np.isfinite(target).all()
                    or abs(float(target.sum()) - 1) > 2e-3 or (target < 0).any()):
                raise ValueError("invalid expert target")
            children, weights, self_weight, terminal_value = consistency
            if (len(children) != len(weights) or not np.isfinite(weights).all()
                    or (weights < 0).any() or not 0 <= self_weight <= 1
                    or not np.isfinite(terminal_value)
                    or float(weights.sum()) + self_weight > 1.002):
                raise ValueError("invalid search consistency target")
        self.rows.extend(row for row, *_ in rows)
        self.targets.extend(np.asarray(target, np.float16) for _, target, *_ in rows)
        self.consistencies.extend(row[5] for row in rows)
        self.versions = np.r_[self.versions, np.asarray([row[2] for row in rows], np.int64)]
        self.visits = np.r_[self.visits, np.asarray([row[3] for row in rows], np.int32)]
        self.depths = np.r_[self.depths, np.asarray([row[4] for row in rows], np.int32)]
        self.ids = np.r_[self.ids, np.arange(self.next_id, self.next_id + len(rows))]
        self.next_id += len(rows)
        self.seen += len(rows)
        if len(self) > self.capacity:
            count = len(self) - self.capacity
            self.evicted += count
            self.discard(np.arange(count))

    def sample(self, size, rng):
        return rng.choice(len(self), min(size, len(self)), replace=False)

    def prune(self, version, lag):
        stale = np.flatnonzero(self.versions < version - lag)
        self.stale += len(stale)
        self.discard(stale)

    def discard(self, indices):
        if not len(indices):
            return
        keep = np.ones(len(self), bool); keep[indices] = False
        self.rows = [row for row, selected in zip(self.rows, keep) if selected]
        self.targets = [row for row, selected in zip(self.targets, keep) if selected]
        self.consistencies = [row for row, selected in zip(self.consistencies, keep) if selected]
        self.versions = self.versions[keep]
        self.visits = self.visits[keep]
        self.depths = self.depths[keep]
        self.ids = self.ids[keep]

    def discard_ids(self, ids):
        self.discard(np.flatnonzero(np.isin(self.ids, ids)))


def train_stream(model, optimizer, args, sampler_session, stage, target, deadline, budget,
                 base_decisions, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
                 save_report, save_step, fingerprint, critic_balance, revisions):
    ascension, bonus = STAGES[stage]
    collector_args = copy.copy(args)
    collector_args.cache_features = (
        args.freeze_backbone and args.target_kl >= 1
        and not args.winning_capacity
    )
    pending_capacity = max(1, reservoir.capacity // args.envs)
    collector_args.envs //= args.samplers
    dataset = ExperienceDataset()
    expert_dataset = ExpertDataset(args.expert_capacity)
    episodes = [[] for _ in range(5)]
    promotion_episodes = []
    losses = {key: [] for key in (
        "mean_advantage", "policy_loss", "expert_loss", "expert_entropy", "expert_kl",
        "expert_rows", "ppo_policy_head_grad_norm", "expert_policy_head_grad_norm",
        "expert_ppo_grad_ratio", "expert_ppo_grad_cosine", "critic_loss",
        "critic_explained_reward_variance", "search_consistency_loss", "critic_expected",
        "critic_win_probability", "entropy", "entropy_weight", "kl",
        "post_kl", "clip_fraction", "winning_loss", "winning_kl",
    )}
    floor_explained_sum = np.zeros(MAX_PROGRESS + 1)
    floor_explained_weight = np.zeros(MAX_PROGRESS + 1)
    floor_explained_rows = np.zeros(MAX_PROGRESS + 1, np.int64)
    def record_floor_explained(metrics):
        for floor, metric in metrics.items():
            index, rows = int(floor), metric["rows"]
            weight = rows * metric["target_variance"]
            floor_explained_sum[index] += metric["value"] * weight
            floor_explained_weight[index] += weight
            floor_explained_rows[index] += rows
    pipeline = [
        f"{args.samplers} continuous CPU actor{'s' if args.samplers > 1 else ''} → "
        + (f"{args.segment_steps}-decision bootstrapped segments" if args.segment_steps
           else "complete terminal trajectories"),
        "Bounded queue → trajectory-level action-ratio freshness filter",
        f"{'Character-balanced' if args.character_balanced else 'Uniform'} reusable rows; "
        f"prefilter forced; FIFO capacity {args.dataset_capacity} by arrival ID; "
        f"ratio-invalid rows reject their trajectory; "
        f"rejected batches refill to {args.batch} rows; priority -{args.priority_decay:g} per use",
        f"{model.layers}-layer global Transformer over state, entity, and action tokens → heads",
        f"Terminal progress shaped by phi=current floor + resource potential → "
        f"GAE γ={args.gae_gamma:g}, λ={args.gae_lambda:g}; "
        "scalar critic predicts the shaped residual",
        "Critic loss balanced by EMA character/phase/canonical-floor frequency",
        "Turn-start native MCTS → expectimax-Q targets and policy-expectation critic transitions",
        ("Frozen encoder and policy; critic head only" if args.critic_only else
         "Asynchronous clipped PPO; policy and value heads updated every iteration"
         if args.freeze_backbone else
         "Asynchronous clipped PPO; full model updated every iteration") + "; "
        f"policy-head LR ×{args.head_learning_rate_multiplier:g}; "
        f"critic LR ×{args.critic_learning_rate_multiplier:g}; "
        f"weights published every {args.publish_updates} updates",
    ]
    winning_behavior_drift = 0
    decisions = discarded_steps = handled = sampled = attempted = forced = trained = updates = windows = 0
    mcts_roots = mcts_simulations = mcts_leaves = mcts_nodes = mcts_batches = mcts_targets = 0
    mcts_turn_starts = 0
    mcts_seconds = mcts_simulate_seconds = mcts_encode_seconds = 0.0
    mcts_inference_seconds = mcts_backup_seconds = mcts_rollout_seconds = 0.0
    mcts_rollout_steps = mcts_rollout_completed = mcts_rollout_invalid = mcts_timeouts = 0
    expert_visits = expert_depth = 0
    segmented_trajectories = 0
    winning_added = winning_replayed = winning_rejected = orphan_empty_actions = post_kl_checks = 0
    policy_update_attempts = pre_kl_rejected_updates = post_kl_discarded_updates = 0
    post_kl_proposals = post_kl_rejected_proposals = post_kl_retries = post_kl_retry_depth = 0
    critic_only_updates = 0
    accepted_pre_kl_sum = accepted_post_kl_sum = 0.0
    winning_replayed_characters = [0] * 5
    winning_evicted_characters = [0] * 5
    trajectory_lengths = []
    trajectory_policy_spans = []
    trajectory_arrival_lags = []
    trajectory_stale_steps = []
    trajectory_seconds = []
    update_durations = []
    unpack_durations = []
    forward_durations = []
    backward_durations = []
    collect_seconds = update_seconds = screen_seconds = 0.0
    screen_unpack_seconds = screen_forward_seconds = 0.0
    collector_seconds = [0.0] * args.samplers
    worker_accounted = [0] * args.samplers
    worker_resolved = [0] * args.samplers
    sampler_generations = [0] * args.samplers
    sampler_restarts = [0] * args.samplers
    sampler_wedges = [0] * args.samplers
    sampler_restart_streaks = [0] * args.samplers
    sampler_recovery_packets = [0] * args.samplers
    sampler_stale_since = [None] * args.samplers
    sampler_exhausted = [False] * args.samplers
    watchdog_dropped = queue_full_waits = 0
    queue_put_seconds = queue_delay_sum = 0.0
    queue_packets = queue_peak = 0
    cache_stats = dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0)
    observed_kl = observed_clip = 0.0
    latest_sampler_version = revisions["policy_revision"]
    dataset_peak = 0
    sampler_versions = [revisions["policy_revision"]] * args.samplers
    sampler_iterations = [base_decisions // args.envs] * args.samplers
    latest_sampler_iteration = base_decisions // args.envs
    policy_lags = []
    reported_episodes = [0] * 5
    reported_metrics = {key: 0 for key in losses}
    reported_steps = reported_trajectories = 0
    reported_seconds = 0.0
    started = time.monotonic()
    next_heartbeat_log = started
    rng = np.random.default_rng(args.seed + sampler_session + 1_000_000_000)
    threaded = args.sampler_backend == "thread"
    context = multiprocessing.get_context(os.environ.get("SAMPLER_START", "spawn"))
    worker_type = threading.Thread if threaded else context.Process
    queue_type = Queue if threaded else context.Queue
    if threaded:
        os.environ["RAYON_NUM_THREADS"] = str(args.samplers * args.sampler_threads)
    models = [queue_type(maxsize=1) for _ in range(args.samplers)]
    sample_capacity = max(8, args.samplers * 16)
    sample_source = queue_type(maxsize=sample_capacity)
    samples = sample_source if threaded else Queue(maxsize=sample_capacity)
    results = queue_type(); stop = threading.Event() if threaded else context.Event()
    heartbeat = [started] * args.samplers if threaded else context.Array("d", [started] * args.samplers)
    progress = [0] * args.samplers if threaded else context.Array("q", [0] * args.samplers)
    packer = ThreadPoolExecutor(max_workers=1)
    workers = [None] * args.samplers
    watchdog_terminated = set()
    watchdog_pending = [False] * args.samplers
    watchdog_failures = Queue()
    actor = export_value_model(None, model, fingerprint, 1, 0, True)
    revisions["actor_revision"] += 1
    actor_revision = revisions["actor_revision"]
    policy_version = revisions["policy_revision"]
    emit_event({
        "event": "actor_published", "actor_revision": actor_revision,
        "weights_revision": revisions["weights_revision"],
        "policy_revision": policy_version, "step": base_decisions,
        "resolved_decisions_total": base_decisions, "stage": stage,
        "training_elapsed_seconds": time.monotonic() - run_started,
    })

    def receive_samples():
        while not stop.is_set():
            try:
                item = sample_source.get(timeout=.1)
            except (Empty, EOFError, OSError):
                continue
            while not stop.is_set():
                try:
                    samples.put(item, timeout=.1)
                    break
                except Full:
                    pass

    receiver = None if threaded else threading.Thread(
        target=receive_samples, name="sample-receiver", daemon=True,
    )

    def start_worker(worker):
        heartbeat[worker] = time.monotonic()
        progress[worker] = 0
        sampler_recovery_packets[worker] = 0
        sampler_stale_since[worker] = None
        process = worker_type(target=collect_worker, args=(
            actor, collector_args, sampler_session, stage,
            reservoir.capacity, pending_capacity, sampler_iterations[worker], worker,
            sampler_generations[worker], actor_revision, policy_version, models[worker], sample_source,
            stop, deadline, budget, results, heartbeat, progress, os.getpid(),
        ), name=f"sampler-{worker}", daemon=True)
        workers[worker] = process
        process.start()

    def publish():
        nonlocal actor, actor_revision, policy_version
        if stop.is_set():
            return
        actor = export_value_model(None, model, fingerprint, 1, 0, True)
        revisions["actor_revision"] += 1
        actor_revision = revisions["actor_revision"]
        policy_version = revisions["policy_revision"]
        emit_event({
            "event": "actor_published", "actor_revision": actor_revision,
            "weights_revision": revisions["weights_revision"],
            "policy_revision": policy_version, "step": base_decisions + handled,
            "resolved_decisions_total": base_decisions + handled, "stage": stage,
            "training_elapsed_seconds": time.monotonic() - run_started,
        })
        for queue in models:
            try:
                queue.put_nowait((actor_revision, policy_version, actor))
            except Full:
                try:
                    queue.get(timeout=.1)
                    queue.put((actor_revision, policy_version, actor), timeout=.1)
                except (Empty, Full, EOFError, OSError):
                    pass

    def ingest(item, packet=True):
        nonlocal decisions, handled, forced, discarded_steps, sampled, collect_seconds, winning_added, orphan_empty_actions, latest_sampler_version, latest_sampler_iteration, dataset_peak, segmented_trajectories, queue_full_waits, queue_put_seconds, queue_delay_sum, queue_packets, queue_peak, mcts_roots, mcts_simulations, mcts_leaves, mcts_nodes, mcts_batches, mcts_targets, mcts_turn_starts, mcts_seconds, mcts_simulate_seconds, mcts_encode_seconds, mcts_inference_seconds, mcts_backup_seconds, mcts_rollout_steps, mcts_rollout_completed, mcts_rollout_invalid, mcts_rollout_seconds, mcts_timeouts
        worker, generation, version, result = item
        if generation != sampler_generations[worker]:
            return
        if packet:
            sampler_recovery_packets[worker] += 1
            if sampler_recovery_packets[worker] >= 2:
                sampler_restart_streaks[worker] = 0
        sampler_versions[worker] = version
        sampler_iterations[worker] = result["iteration"]
        latest_sampler_version = min(sampler_versions)
        latest_sampler_iteration = max(sampler_iterations)
        added, excluded, excess = dataset.add(result, args, budget - decisions)
        capacity_dropped = dataset.trim(args.dataset_capacity)
        expert_dataset.add(result.get("expert_rows", []))
        decisions += added
        handled += excluded + capacity_dropped
        forced += excluded
        if decisions >= budget:
            stop.set()
        sampled += result["sampled_steps"]
        worker_accounted[worker] += result["sampled_steps"]
        worker_resolved[worker] += result["discarded_steps"] + sum(
            len(trajectory["rows"]) for trajectory in result["trajectories"]
        )
        discarded_steps += result["discarded_steps"]
        discarded_steps += excess
        mcts_roots += result.get("mcts_roots", 0)
        mcts_simulations += result.get("mcts_simulations", 0)
        mcts_leaves += result.get("mcts_leaves", 0)
        mcts_nodes += result.get("mcts_nodes", 0)
        mcts_batches += result.get("mcts_batches", 0)
        mcts_targets += result.get("mcts_targets", 0)
        mcts_turn_starts += result.get("mcts_turn_starts", 0)
        mcts_seconds += result.get("mcts_seconds", 0.0)
        mcts_simulate_seconds += result.get("mcts_simulate_seconds", 0.0)
        mcts_encode_seconds += result.get("mcts_encode_seconds", 0.0)
        mcts_inference_seconds += result.get("mcts_inference_seconds", 0.0)
        mcts_backup_seconds += result.get("mcts_backup_seconds", 0.0)
        mcts_rollout_steps += result.get("mcts_rollout_steps", 0)
        mcts_rollout_completed += result.get("mcts_rollout_completed", 0)
        mcts_rollout_invalid += result.get("mcts_rollout_invalid", 0)
        mcts_rollout_seconds += result.get("mcts_rollout_seconds", 0.0)
        mcts_timeouts += result.get("mcts_timeouts", 0)
        for trajectory in result["trajectories"]:
            segmented_trajectories += int(not trajectory["terminals"][-1])
            trajectory_lengths.append(len(trajectory["rows"]))
            trajectory_policy_spans.append(int(
                max(trajectory["versions"]) - min(trajectory["versions"])
            ))
            trajectory_arrival_lags.append(int(max(0, updates - min(trajectory["versions"]))))
            trajectory_stale_steps.append(int(sum(
                version < updates - args.max_policy_lag for version in trajectory["versions"]
            )))
            trajectory_seconds.append(trajectory["completion_seconds"])
        dataset_peak = max(dataset_peak, len(dataset))
        update = result["reservoir"]
        missing = [index for index, row in enumerate(update["rows"]) if row[4] is None]
        for start in range(0, len(missing), 128):
            indices = missing[start:start + 128]
            with torch.no_grad():
                inputs = unpack([update["rows"][index][0] for index in indices], target, model)
                logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[0].masked_fill(~inputs[6], -torch.inf)
                policies = logits.log_softmax(-1).cpu().numpy()
            for index, policy in zip(indices, policies):
                row = update["rows"][index]
                update["rows"][index] = (*row[:4], np.asarray(
                    policy[:packed_action_count(row[0])], np.float16,
                ))
        packet_winning_added = reservoir.admit(
            update["rows"], update["wins"], update["skipped"], update["forced"], rng,
        )
        winning_added += packet_winning_added
        for target_episodes, collected in zip(episodes, result["episodes"]):
            target_episodes.extend(collected)
        for character, collected in enumerate(result["episodes"]):
            promotion_episodes.extend((character, row) for row in collected)
        collector_seconds[worker] += result["collect_seconds"]
        collect_seconds = max(collector_seconds)
        for key in cache_stats:
            cache_stats[key] += result[key]
        orphan_empty_actions += result["orphan_empty_actions"]
        queue_full_waits += result.get("queue_full_waits", 0)
        queue_put_seconds += result.get("queue_put_seconds", 0)
        queue_delay = None
        if "queued_at" in result:
            queue_delay = time.monotonic() - result["queued_at"]
            queue_delay_sum += queue_delay
            queue_packets += 1
        try:
            queue_peak = max(queue_peak, samples.qsize())
        except NotImplementedError:
            pass
        emit_event({
            "event": "sample_packet", "step": base_decisions + handled,
            "resolved_decisions_total": base_decisions + handled,
            "sampled_decisions_total": base_decisions + sampled,
            "training_elapsed_seconds": time.monotonic() - run_started,
            "stage": stage, "worker": worker, "generation": generation,
            "weights_revision": revisions["weights_revision"],
            "policy_revision": revisions["policy_revision"],
            "packet_policy_revision": version,
            "packet_sampled_decisions": result["sampled_steps"],
            "trajectory_rows": sum(len(row["rows"]) for row in result["trajectories"]),
            "accepted_rows": added, "admitted_rows": added - excluded,
            "forced_rows": excluded, "budget_excess_rows": excess,
            "capacity_dropped_rows": capacity_dropped,
            "discarded_decisions": result["discarded_steps"] + excess,
            "expert_rows": len(result.get("expert_rows", ())),
            "winning_candidates": len(update["rows"]),
            "winning_admitted": packet_winning_added,
            "queue_delay_seconds": queue_delay,
            "queue_put_seconds": result.get("queue_put_seconds", 0.0),
            "collect_seconds": result.get("collect_seconds", 0.0),
            "cache": {
                "card_hits": result.get("card_hit", 0),
                "card_misses": result.get("card_miss", 0),
                "graph_hits": result.get("graph_hit", 0),
                "graph_misses": result.get("graph_miss", 0),
            },
            "mcts": {
                key.removeprefix("mcts_"): result.get(key, 0)
                for key in (
                    "mcts_roots", "mcts_simulations", "mcts_leaves", "mcts_nodes",
                    "mcts_batches", "mcts_targets", "mcts_turn_starts", "mcts_seconds",
                    "mcts_simulate_seconds", "mcts_encode_seconds",
                    "mcts_inference_seconds", "mcts_backup_seconds",
                    "mcts_rollout_steps", "mcts_rollout_completed",
                    "mcts_rollout_invalid", "mcts_rollout_seconds", "mcts_timeouts",
                )
            },
            "episodes": result.get("completions", ()),
        }, console=False)

    def drain_samples():
        for _ in range(sample_capacity):
            try:
                ingest(samples.get_nowait())
            except (Empty, EOFError, OSError):
                break

    def prepare(rows):
        prepared = time.monotonic()
        return unpack(rows, torch.device("cpu"), model, False), time.monotonic() - prepared

    def prepare_cached(features):
        prepared = time.monotonic()
        return cached_tensors(features, model.width), time.monotonic() - prepared

    def reserve_batch(size):
        if len(refill_ids):
            ids = dataset.data["id"][:len(dataset)]
            retained = np.isin(ids, refill_ids)
            kept = np.flatnonzero(retained)
            selected = np.concatenate((kept, dataset.sample(
                size - len(kept), rng, args.character_balanced,
                np.flatnonzero(~retained),
            )))
        else:
            selected = dataset.sample(size, rng, args.character_balanced)
        rows = [dataset.rows[index] for index in selected]
        expert_selected = expert_dataset.sample(args.expert_batch, rng) \
            if args.expert_weight or args.search_consistency_weight else np.empty(0, np.int64)
        expert_rows = [expert_dataset.rows[index] for index in expert_selected]
        expert_targets = [expert_dataset.targets[index] for index in expert_selected]
        expert_ids = expert_dataset.ids[expert_selected].copy()
        expert_visits_batch = expert_dataset.visits[expert_selected].copy()
        expert_depths_batch = expert_dataset.depths[expert_selected].copy()
        consistency_positions = np.asarray([
            index for index, selected in enumerate(expert_selected)
            if expert_dataset.consistencies[selected][2] < 1
        ], np.int64)
        if len(consistency_positions) > args.search_consistency_batch:
            consistency_positions = rng.choice(
                consistency_positions, args.search_consistency_batch, replace=False
            )
        search_children = []
        search_groups = []
        for group, position in enumerate(consistency_positions):
            children, weights, self_weight, terminal_value = \
                expert_dataset.consistencies[expert_selected[position]]
            search_groups.append((position, group, len(children), weights,
                                  self_weight, terminal_value))
            search_children.extend(children)
        values = {key: dataset.data[key][selected].copy() for key in dataset.data}
        replay = reservoir.sample(len(selected) // 9, rng)
        cached = bool(dataset.features) and not expert_rows and not replay and not search_children
        return selected, rows, expert_ids, expert_rows, expert_targets, \
            expert_visits_batch, expert_depths_batch, values, replay, search_groups, \
            search_children, cached, packer.submit(
            prepare_cached, [dataset.features[index] for index in selected]
        ) if cached else packer.submit(
            prepare, rows + expert_rows + [sample[0] for sample in replay] + search_children
        )

    def optimizer_metrics():
        return {
            "policy_rows_attempted": attempted,
            "policy_rows_forced": forced,
            "policy_rows_accepted": trained,
            "policy_rows_discarded": attempted - trained,
            "ratio_rejected_rows": dataset.ratio_dropped,
            "pre_kl_rejected_rows": dataset.kl_dropped,
            "post_kl_discarded_rows": dataset.post_kl_dropped,
            "policy_update_attempts": policy_update_attempts,
            "policy_updates_accepted": updates,
            "pre_kl_rejected_updates": pre_kl_rejected_updates,
            "post_kl_proposals": post_kl_proposals,
            "post_kl_rejected_proposals": post_kl_rejected_proposals,
            "post_kl_retries": post_kl_retries,
            "post_kl_retry_depth_max": post_kl_retry_depth,
            "post_kl_discarded_updates": post_kl_discarded_updates,
            "critic_only_updates": critic_only_updates,
            "accepted_pre_kl_sum": accepted_pre_kl_sum,
            "accepted_post_kl_sum": accepted_post_kl_sum,
            "accepted_kl_count": updates,
            "dataset_ultimately_discarded": attempted - trained,
        }

    def report():
        nonlocal reported_steps, reported_seconds, reported_trajectories
        recent_by_character = [rows[reported_episodes[index]:] for index, rows in enumerate(episodes)]
        recent = [episode for rows in recent_by_character for episode in rows]
        recent_summary = episode_summary(recent)
        reported_episodes[:] = map(len, episodes)
        elapsed = time.monotonic() - run_started
        segment_elapsed = time.monotonic() - started
        recent_metrics = metric_means({
            key: values[reported_metrics[key]:]
            for key, values in losses.items() if len(values) > reported_metrics[key]
        })
        recent_lengths = trajectory_lengths[reported_trajectories:]
        recent_spans = trajectory_policy_spans[reported_trajectories:]
        recent_arrival_lags = trajectory_arrival_lags[reported_trajectories:]
        recent_stale_steps = trajectory_stale_steps[reported_trajectories:]
        recent_seconds = trajectory_seconds[reported_trajectories:]
        reported_trajectories = len(trajectory_lengths)
        for key, values in losses.items():
            reported_metrics[key] = len(values)
        replay_fraction = winning_replayed / max(1, trained + winning_replayed)
        assert replay_fraction <= .1 + 1e-9
        _, _, curriculum = curriculum_weights(
            stage, stage_decisions + handled, auxiliary_decisions + handled, args, progress_active
        )
        point = {
            "steps": base_decisions + handled, "sampled_decisions": base_decisions + sampled,
            "accepted_decisions": base_decisions + decisions,
            "iteration": latest_sampler_iteration, "samplers": args.samplers,
            "updates": updates, "episodes": len(recent),
            "wins": sum(row[0] for row in recent), "caps": sum(row[2] or row[3] or row[4] for row in recent),
            "win_rate": recent_summary["win_rate"],
            "win_rate_interval": recent_summary["win_rate_interval"],
            "boss_entries": recent_summary["boss_entries"],
            "boss_entry_rate": recent_summary["boss_entry_rate"],
            "boss_entry_rate_interval": recent_summary["boss_entry_rate_interval"],
            "boss_conversion": recent_summary["boss_conversion"],
            "boss_conversion_interval": recent_summary["boss_conversion_interval"],
            "seconds": elapsed,
            "decisions_per_second": (handled - reported_steps) / max(1e-9, segment_elapsed - reported_seconds),
            "trajectory_floors": [(row[5], row[1], character)
                                  for character, rows in enumerate(recent_by_character) for row in rows],
            "step_caps": sum(row[2] for row in recent), "combat_caps": sum(row[3] for row in recent),
            "empty_actions": sum(row[4] for row in recent),
            "collect_seconds": collect_seconds, "screen_seconds": screen_seconds,
            "screen_unpack_seconds": screen_unpack_seconds,
            "screen_forward_seconds": screen_forward_seconds,
            "update_seconds": update_seconds,
            "actor_decisions_per_second": sampled / max(1e-9, collect_seconds),
            "learner_decisions_per_second": trained / max(1e-9, update_seconds),
            "mcts_roots": mcts_roots, "mcts_simulations": mcts_simulations,
            "mcts_leaves": mcts_leaves, "mcts_nodes": mcts_nodes,
            "mcts_batches": mcts_batches, "mcts_targets": mcts_targets,
            "mcts_turn_starts": mcts_turn_starts,
            "mcts_seconds": mcts_seconds,
            "mcts_root_fraction": mcts_roots / max(1, mcts_turn_starts),
            "mcts_roots_per_decision": mcts_roots / max(1, sampled),
            "mcts_simulations_per_root": mcts_simulations / max(1, mcts_roots),
            "mcts_simulations_per_second": mcts_simulations / max(1e-9, mcts_seconds),
            "mcts_leaf_batch_mean": mcts_leaves / max(1, mcts_batches),
            "mcts_targets_per_root": mcts_targets / max(1, mcts_roots),
            "mcts_simulate_fraction": mcts_simulate_seconds / max(1e-9, mcts_seconds),
            "mcts_encode_fraction": mcts_encode_seconds / max(1e-9, mcts_seconds),
            "mcts_inference_fraction": mcts_inference_seconds / max(1e-9, mcts_seconds),
            "mcts_backup_fraction": mcts_backup_seconds / max(1e-9, mcts_seconds),
            "mcts_rollout_steps": mcts_rollout_steps,
            "mcts_rollout_completed": mcts_rollout_completed,
            "mcts_rollout_invalid": mcts_rollout_invalid,
            "mcts_timeouts": mcts_timeouts,
            "mcts_rollout_fraction": mcts_rollout_seconds / max(1e-9, mcts_seconds),
            "expert_buffer_rows": len(expert_dataset),
            "expert_rows_seen": expert_dataset.seen,
            "expert_rows_used": expert_dataset.used,
            "expert_rows_stale": expert_dataset.stale,
            "expert_rows_evicted": expert_dataset.evicted,
            "expert_visit_mean": expert_visits / max(1, expert_dataset.used),
            "expert_depth_mean": expert_depth / max(1, expert_dataset.used),
            "row_utilization": trained / max(1, attempted),
            "update_seconds_p95": float(np.quantile(update_durations, .95)) if update_durations else 0,
            "unpack_seconds_p95": float(np.quantile(unpack_durations, .95)) if unpack_durations else 0,
            "forward_seconds_p95": float(np.quantile(forward_durations, .95)) if forward_durations else 0,
            "backward_seconds_p95": float(np.quantile(backward_durations, .95)) if backward_durations else 0,
            **cache_stats,
            "card_cache_hit_rate": cache_stats["card_hit"] / max(
                1, cache_stats["card_hit"] + cache_stats["card_miss"]
            ),
            "graph_cache_hit_rate": cache_stats["graph_hit"] / max(
                1, cache_stats["graph_hit"] + cache_stats["graph_miss"]
            ),
            "mps_allocated_bytes": torch.mps.current_allocated_memory() if target.type == "mps" else 0,
            "mps_driver_allocated_bytes": torch.mps.driver_allocated_memory() if target.type == "mps" else 0,
            "floor_bands": bands([row[1] for row in recent]),
            "dataset_rows": len(dataset), "dataset_peak": dataset_peak,
            "dataset_seen": dataset.seen, "dataset_admitted": dataset.admitted,
            "dataset_uses": dataset.uses, "dataset_retired": dataset.retired,
            "dataset_forced_dropped": dataset.forced_dropped,
            "dataset_capacity_dropped": dataset.capacity_dropped,
            "dataset_incomplete_dropped": dataset.incomplete_dropped,
            "dataset_priority_mean": float(dataset.data["priority"][:len(dataset)].mean()) if len(dataset) else 0,
            "dataset_priority_max": float(dataset.data["priority"][:len(dataset)].max()) if len(dataset) else 0,
            "dataset_attempted": attempted, "dataset_trained": trained,
            "dataset_stale_dropped": dataset.stale_dropped,
            "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
            "dataset_post_kl_dropped": dataset.post_kl_dropped,
            "post_kl_checks": post_kl_checks,
            **optimizer_metrics(),
            "discarded_steps": discarded_steps,
            "discarded_step_fraction": discarded_steps / max(1, sampled),
            "completed_trajectories": len(trajectory_lengths),
            "bootstrapped_segments": segmented_trajectories,
            "trajectory_length_mean": float(np.mean(recent_lengths)) if recent_lengths else 0,
            "trajectory_length_max": max(recent_lengths, default=0),
            "trajectory_policy_span_mean": float(np.mean(recent_spans)) if recent_spans else 0,
            "trajectory_policy_span_max": max(recent_spans, default=0),
            "trajectory_arrival_lag_mean": float(np.mean(recent_arrival_lags)) if recent_arrival_lags else 0,
            "trajectory_arrival_lag_max": max(recent_arrival_lags, default=0),
            "trajectory_stale_step_fraction": sum(recent_stale_steps) / max(1, sum(recent_lengths)),
            "trajectory_completion_seconds_mean": float(np.mean(recent_seconds)) if recent_seconds else 0,
            "trajectory_completion_seconds_max": max(recent_seconds, default=0),
            "policy_version": revisions["policy_revision"],
            "weights_revision": revisions["weights_revision"],
            "actor_revision": revisions["actor_revision"],
            "sampler_version": latest_sampler_version,
            "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
            "sampler_heartbeats": [time.monotonic() - value for value in heartbeat[:]],
            "sampler_restarts": sampler_restarts, "sampler_wedges": sampler_wedges,
            "sampler_restart_streaks": sampler_restart_streaks,
            "watchdog_dropped_steps": watchdog_dropped,
            "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
            "sample_queue_packets": queue_packets,
            "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
            "sample_queue_full_waits": queue_full_waits,
            "sample_queue_put_seconds": queue_put_seconds,
            "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
            "winning_character_rows": list(map(len, reservoir.by_character())),
            "winning_episodes": reservoir.wins, "winning_added": winning_added,
            "winning_skipped": reservoir.skipped, "winning_forced_skipped": reservoir.forced,
            "winning_replayed": winning_replayed,
            "winning_rejected_kl": winning_rejected,
            "winning_replayed_characters": winning_replayed_characters,
            "winning_evicted_characters": winning_evicted_characters,
            "winning_replay_fraction": replay_fraction,
            "winning_loss_weight": args.winning_loss_weight,
            "winning_collection_drift": float(winning_behavior_drift),
            "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
            "progress_active": progress_active, "curriculum": curriculum,
            "critic_floor_conditioned_explained_reward_variance":
                floor_explained_sum.sum() / floor_explained_weight.sum()
                if floor_explained_weight.any() else 0.,
            "critic_explained_reward_variance_by_floor": {
                str(floor): {
                    "value": floor_explained_sum[floor] / floor_explained_weight[floor]
                    if floor_explained_weight[floor] else 0.,
                    "target_variance": floor_explained_weight[floor] / rows,
                    "rows": int(rows),
                }
                for floor, rows in enumerate(floor_explained_rows) if rows
            },
            **critic_balance.report(),
            **recent_metrics,
        }
        reported_steps = handled
        reported_seconds = segment_elapsed
        point["characters"] = summaries([rows[-args.promotion_window:] for rows in episodes])
        save_report(point, pipeline, windows)
        floor_explained_sum.fill(0); floor_explained_weight.fill(0); floor_explained_rows.fill(0)
    next_report = base_decisions + args.report_decisions
    next_save = base_decisions + args.save_decisions
    promotion_ready = False
    promotion_result = None
    def update_promotion():
        nonlocal promotion_ready, promotion_result
        if args.critic_only or promotion_ready or stage + 1 >= len(STAGES):
            return
        promotion_ready, promotion_result = promotion_sample(
            promotion_episodes, args.promotion_window, args.promote_win_rate,
        )
    pending = []
    refill_ids = np.empty(0, np.int64)
    def drain_results():
        while True:
            try:
                ingest(results.get_nowait(), False)
            except (Empty, EOFError, OSError):
                return

    def training_batch_event(policy_outcome, commit_kind, **values):
        emit_event({
            "event": "training_batch", "policy_outcome": policy_outcome,
            "commit_kind": commit_kind, "step": base_decisions + handled,
            "sampled_decisions_total": base_decisions + sampled,
            "update_attempt": revisions["update_attempt"],
            "weights_revision": revisions["weights_revision"],
            "policy_revision": revisions["policy_revision"], "stage": stage,
            "resolved_decisions_total": base_decisions + handled,
            "training_elapsed_seconds": time.monotonic() - run_started,
            **values,
        })

    def watch_samplers():
        while not stop.wait(min(1., args.sampler_timeout / 10)):
            now = time.monotonic()
            if now >= deadline:
                return
            for worker, process in enumerate(workers):
                if sampler_exhausted[worker] or watchdog_pending[worker]:
                    continue
                alive = process.is_alive()
                stale = alive and now - heartbeat[worker] > args.sampler_timeout
                if not stale:
                    sampler_stale_since[worker] = None
                elif sampler_stale_since[worker] is None:
                    sampler_stale_since[worker] = now
                wedged = alive and sampler_wedged(
                    heartbeat[worker], sampler_stale_since[worker], now, args.sampler_timeout,
                )
                failed = not alive and (threaded or process.exitcode not in (None, 0))
                if not wedged and not failed:
                    continue
                watchdog_pending[worker] = True
                if threaded:
                    stop.set()
                elif wedged:
                    watchdog_terminated.add(process)
                    process.terminate(); process.join(5)
                    if process.is_alive():
                        process.kill(); process.join()
                watchdog_failures.put((
                    worker, process, wedged, None if threaded else process.exitcode,
                ))

    def recover_workers():
        nonlocal sampled, discarded_steps, watchdog_dropped
        while True:
            try:
                worker, process, wedged, exitcode = watchdog_failures.get_nowait()
            except Empty:
                return
            if process is not workers[worker]:
                watchdog_pending[worker] = False
                continue
            if threaded:
                stop.set(); sampler_exhausted[worker] = True; watchdog_pending[worker] = False
                continue
            if wedged:
                sampler_wedges[worker] += 1
            process.join(5)
            if process.is_alive():
                process.kill(); process.join()
            drain_samples()
            drain_results()
            observed = int(progress[worker])
            sampled += max(0, observed - worker_accounted[worker])
            dropped = max(0, observed - worker_resolved[worker])
            discarded_steps += dropped
            watchdog_dropped += dropped
            worker_accounted[worker] = worker_resolved[worker] = 0
            emit_event({
                "event": "sampler_failure", "worker": worker,
                "generation": sampler_generations[worker], "wedged": wedged,
                "exitcode": exitcode, "dropped": dropped,
            }, "WARNING")
            if (sampler_restart_streaks[worker] >= args.sampler_restarts or stop.is_set()
                    or time.monotonic() >= deadline or decisions >= budget):
                sampler_exhausted[worker] = True
                watchdog_pending[worker] = False
                continue
            sampler_restarts[worker] += 1
            sampler_restart_streaks[worker] += 1
            sampler_generations[worker] += 1
            while True:
                try:
                    models[worker].get_nowait()
                except (Empty, EOFError, OSError):
                    break
            start_worker(worker)
            watchdog_pending[worker] = False
            emit_event({
                "event": "sampler_restart", "worker": worker,
                "generation": sampler_generations[worker],
                "restarts": sampler_restarts[worker], "streak": sampler_restart_streaks[worker],
            })

    watchdog = threading.Thread(target=watch_samplers, name="sampler-watchdog", daemon=True)
    try:
        if receiver:
            receiver.start()
        for worker in range(args.samplers):
            start_worker(worker)
        watchdog.start()
        while True:
            drain_results()
            drain_samples()
            recover_workers()
            now = time.monotonic()
            if now >= next_heartbeat_log:
                try:
                    queue_size = samples.qsize()
                except NotImplementedError:
                    queue_size = -1
                emit_event({
                    "event": "heartbeat", "step": base_decisions + handled,
                    "resolved_decisions_total": base_decisions + handled,
                    "sampled_decisions_total": base_decisions + sampled,
                    "accepted_decisions_total": base_decisions + decisions,
                    "policy_trained_rows_total": trained,
                    "discarded_decisions_total": discarded_steps,
                    "update_attempt": revisions["update_attempt"],
                    "weights_revision": revisions["weights_revision"],
                    "policy_revision": revisions["policy_revision"],
                    "actor_revision": revisions["actor_revision"],
                    "stage": stage, "dataset_rows": len(dataset),
                    "dataset_peak_rows": dataset_peak, "expert_rows": len(expert_dataset),
                    "winning_rows": len(reservoir.rows), "queue_depth": queue_size,
                    "queue_capacity": sample_capacity,
                    "sampler_policy_revision": latest_sampler_version,
                    "sampler_age_seconds": [now - value for value in heartbeat[:]],
                    "sampler_generations": sampler_generations,
                    "sampler_restarts": sampler_restarts,
                    "sampler_wedges": sampler_wedges,
                    "accelerator_allocated_bytes": (
                        torch.mps.current_allocated_memory() if target.type == "mps" else 0
                    ),
                    "accelerator_driver_allocated_bytes": (
                        torch.mps.driver_allocated_memory() if target.type == "mps" else 0
                    ),
                    "training_elapsed_seconds": time.monotonic() - run_started,
                })
                next_heartbeat_log = now + 10
            sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
            if not pending:
                if len(dataset) < args.batch and not sampler_done:
                    try:
                        ingest(samples.get(timeout=.1))
                        drain_samples()
                    except (Empty, EOFError, OSError):
                        recover_workers()
                        continue
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
                absolute = base_decisions + handled
                expert_stale = len(expert_dataset)
                expert_dataset.prune(revisions["policy_revision"], args.expert_max_lag)
                expert_stale -= len(expert_dataset)
                if expert_stale:
                    emit_event({
                        "event": "dataset_pruned", "step": base_decisions + handled,
                        "resolved_decisions_total": base_decisions + handled,
                        "stage": stage, "stale_rows": 0,
                        "expert_stale_rows": expert_stale,
                        "weights_revision": revisions["weights_revision"],
                        "policy_revision": revisions["policy_revision"],
                        "training_elapsed_seconds": time.monotonic() - run_started,
                    })
                absolute = base_decisions + handled
                if absolute >= next_save:
                    save_step(absolute)
                    next_save += args.save_decisions
                if absolute >= next_report:
                    windows += 1
                    report()
                    next_report += args.report_decisions
                    update_promotion()
                    if promotion_ready:
                        stop.set()
                    continue
                if len(dataset) < args.batch:
                    if sampler_done:
                        remaining = len(dataset)
                        dataset.discard(np.arange(remaining))
                        dataset.incomplete_dropped += remaining
                        handled += remaining
                        break
                    continue
            while len(pending) < 1:
                ingested = False
                if not sampler_done:
                    before = decisions
                    drain_samples()
                    ingested = decisions != before
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
                boundary = min(next_report, next_save) - (base_decisions + handled)
                enough = len(dataset) >= args.batch
                if boundary <= 0:
                    break
                if not enough:
                    if sampler_done or not ingested:
                        break
                    continue
                pending.append(reserve_batch(args.batch))
            if not pending:
                if sampler_done and not len(dataset):
                    break
                continue
            update_started = time.monotonic()
            (selected, rows, expert_ids, expert_rows, expert_targets,
             expert_visits_batch, expert_depths_batch, values, replay, search_groups, search_children,
             cached, packed) = pending.pop(0)
            if not rows:
                continue
            assert len(rows) == args.batch
            revisions["update_attempt"] += 1
            screen_started = time.monotonic()
            unpack_started = time.monotonic()
            cpu_inputs, unpack_elapsed = packed.result()
            screen_unpack_seconds += unpack_elapsed
            inputs = upload(cpu_inputs, target)
            unpack_seconds = time.monotonic() - unpack_started
            unpack_durations.append(unpack_seconds)
            action = torch.as_tensor(values["action"], device=target)
            old = torch.as_tensor(values["old"], device=target)
            lengths = np.asarray([packed_action_count(row) for row in rows], np.int64)
            expert_lengths = np.asarray([
                packed_action_count(row) for row in expert_rows
            ], np.int64)
            choice_index = torch.as_tensor(
                np.cumsum(lengths) - lengths + values["action"], device=target,
            )
            flat_policy = not replay
            forward_started = time.monotonic()
            all_logits, all_critic_logits = (
                predict_cached(model, inputs, args.precision, args.policy_temperature) if cached else
                predict(model, inputs, args.precision, args.policy_temperature, flat_policy=flat_policy)
            )
            forward_seconds = time.monotonic() - forward_started
            screen_forward_seconds += forward_seconds
            screen_seconds += time.monotonic() - screen_started
            forward_durations.append(forward_seconds)
            backward_seconds = 0.0
            critic_logits = all_critic_logits[:len(rows)]
            legal = None if cached else inputs[6][:len(rows)]
            policy_actions = int(lengths.sum())
            expert_actions = int(expert_lengths.sum())
            logits = all_logits[:policy_actions] if flat_policy else all_logits[:len(rows)]
            expert_logits = (all_logits[policy_actions:policy_actions + expert_actions]
                             if flat_policy else
                             all_logits[len(rows):len(rows) + len(expert_rows)])
            log_ratio = (logits[choice_index] if flat_policy else
                         logits.gather(1, action[:, None]).squeeze(1)) - old
            if not cached and not args.critic_only and not (legal.sum(1) > 1).all():
                raise RuntimeError("forced action entered the dataset")
            fresh = torch.ones_like(log_ratio, dtype=torch.bool) if args.critic_only \
                else log_ratio.abs() <= args.max_log_ratio
            invalid = ~fresh.detach().cpu().numpy()
            if invalid.any():
                invalid = np.isin(values["trajectory"], values["trajectory"][invalid])
                refill_ids = values["id"][~invalid]
                ratio_rejected = dataset.discard_trajectories(values["trajectory"][invalid])
                dataset.ratio_dropped += ratio_rejected
                handled += ratio_rejected
                attempted += len(rows)
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                backward_durations.append(0.)
                training_batch_event(
                    "ratio_rejected", "none", attempted_rows=len(rows),
                    fresh_rows=int((~invalid).sum()), policy_trained_rows=0,
                    critic_trained_rows=0, ratio_rejected_rows=ratio_rejected,
                    retired_rows=0, unpack_seconds=unpack_seconds,
                    forward_seconds=forward_seconds, backward_seconds=0.,
                    total_seconds=update_elapsed,
                )
                continue
            refill_ids = np.empty(0, np.int64)
            selected = np.asarray(selected, np.int64)
            expired = dataset.use(selected, args.priority_decay)
            dataset.discard(expired)
            ratio_rejected = 0
            handled += len(expired)
            batch_policy_lags = (
                revisions["policy_revision"] - values["version"][~invalid]
            ).tolist()
            policy_lags.extend(batch_policy_lags)
            mask = fresh.to(log_ratio.dtype)
            fresh_count = mask.sum()
            denominator = fresh_count.clamp_min(1)
            safe_log_ratio = torch.where(fresh, log_ratio, torch.zeros_like(log_ratio))
            ratio = safe_log_ratio.exp()
            kl = ((ratio - 1 - safe_log_ratio) * mask).sum().div(denominator).detach()
            clip_fraction = (((ratio - 1).abs() > args.clip).to(mask.dtype) * mask).sum() \
                .div(denominator).detach()
            fresh_cpu = fresh.detach().cpu().numpy()
            fresh_rows = int(fresh_count)
            attempted += len(rows)
            kl_value = float(kl)
            _, entropy_weight, _ = curriculum_weights(
                stage, stage_decisions + handled, auxiliary_decisions + handled, args, progress_active
            )
            assert fresh_rows == args.batch
            batch_target = torch.as_tensor(values["critic_target"][fresh_cpu], device=target)
            critic_weights = critic_balance.weights(
                values["character"][fresh_cpu], values["phase"][fresh_cpu],
                values["canonical"][fresh_cpu],
            )
            critic_weights = torch.as_tensor(critic_weights, device=target)
            prediction = critic_value(critic_logits[fresh])
            squared_error = (prediction - batch_target).square()
            value_loss = (squared_error * critic_weights).mean()
            critic_loss = args.value_weight * value_loss
            explained_reward_variance, floor_conditioned_explained_reward_variance, \
                explained_reward_variance_by_floor = critic_explained_reward_variance(
                    prediction.detach(), batch_target, values["canonical"][fresh_cpu]
                )
            critic_parameters = tuple(model.critic.parameters())
            critic_balance.record_loss(
                values["terminal"][fresh_cpu],
                (squared_error.detach() * critic_weights).cpu().numpy(),
            )
            set_learning_rate(
                optimizer, args.learning_rate, args.learning_rate_warmup_steps,
                revisions["weights_revision"],
            )
            policy_update_attempts += int(not args.critic_only)
            if not args.critic_only and kl_value > args.target_kl:
                handled += dataset.discard_trajectories(values["trajectory"][fresh_cpu])
                dataset.kl_dropped += fresh_rows
                pre_kl_rejected_updates += 1
                backward_started = time.monotonic()
                optimizer.zero_grad(set_to_none=True); critic_loss.backward(inputs=critic_parameters)
                gradient_norm = critic_only_step(model, optimizer, critic_parameters)
                revisions["weights_revision"] += 1
                if (target.type == "mps" and args.mps_empty_cache_updates
                        and revisions["weights_revision"] % args.mps_empty_cache_updates == 0):
                    torch.mps.empty_cache()
                critic_only_updates += 1
                backward_seconds = time.monotonic() - backward_started
                backward_durations.append(backward_seconds)
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                losses["critic_explained_reward_variance"].append(explained_reward_variance)
                record_floor_explained(explained_reward_variance_by_floor)
                training_batch_event(
                    "pre_kl_rejected", "critic_only", attempted_rows=len(rows),
                    fresh_rows=fresh_rows, policy_trained_rows=0,
                    critic_trained_rows=fresh_rows, ratio_rejected_rows=ratio_rejected,
                    retired_rows=len(expired), critic_loss=float(value_loss.detach()),
                    critic_explained_reward_variance=float(explained_reward_variance),
                    critic_floor_conditioned_explained_reward_variance=
                    floor_conditioned_explained_reward_variance,
                    critic_explained_reward_variance_by_floor=explained_reward_variance_by_floor,
                    advantage_mean=float(np.mean(values["advantage"][fresh_cpu])),
                    advantage_stddev=float(np.std(values["advantage"][fresh_cpu])),
                    policy_lag_mean=float(np.mean(batch_policy_lags)),
                    policy_lag_p95=float(np.quantile(batch_policy_lags, .95)),
                    policy_lag_max=max(batch_policy_lags), pre_kl=kl_value,
                    gradient_norm=gradient_norm,
                    gradient_clipped=gradient_norm > .5, unpack_seconds=unpack_seconds,
                    forward_seconds=forward_seconds, backward_seconds=backward_seconds,
                    total_seconds=update_elapsed,
                )
                publish()
                continue
            advantages = torch.as_tensor(values["advantage"], device=target)
            characters = torch.as_tensor(values["character"], device=target)
            batch_advantage = advantages
            for character in range(5):
                member = fresh & (characters == character)
                member_float = member.to(advantages.dtype)
                count = member_float.sum().clamp_min(1)
                mean = (advantages * member_float).sum() / count
                std = (((advantages - mean).square() * member_float).sum() / count).sqrt()
                batch_advantage = torch.where(
                    member, (advantages - mean) / (std + 1e-8), batch_advantage,
                )
            policy_loss = -(torch.minimum(
                ratio * batch_advantage,
                ratio.clamp(1 - args.clip, 1 + args.clip) * batch_advantage,
            ) * mask).sum() / denominator
            if flat_policy:
                action_row = inputs[3] if cached else inputs[5][0][:len(logits)]
                action_legal = 1 if cached else inputs[5][2][:len(logits)]
                entropy_by_row = logits.new_zeros(len(rows)).index_add(
                    0, action_row, -(logits.exp() * logits * action_legal),
                )
            else:
                entropy_by_row = -(logits.exp() * logits * legal).sum(1)
            entropy = (entropy_by_row * mask).sum() / denominator
            expert_loss = logits.sum() * 0
            expert_entropy = expert_loss
            expert_count = len(expert_rows)
            if expert_count and args.expert_weight:
                if flat_policy:
                    expert_target = np.concatenate(expert_targets).astype(np.float32)
                else:
                    expert_target = np.zeros(expert_logits.shape, np.float32)
                    for row, target_distribution in enumerate(expert_targets):
                        expert_target[row, :len(target_distribution)] = target_distribution
                expert_target = torch.as_tensor(expert_target, device=target)
                positive = expert_target > 0
                expert_loss = -(expert_target[positive] * expert_logits[positive]).sum() \
                    / expert_count
                expert_entropy = -(expert_target[positive] * expert_target[positive].log()).sum() \
                    / expert_count
            search_consistency_loss = critic_logits.sum() * 0
            if search_groups:
                critic_values = critic_value(all_critic_logits)
                child_offset = len(rows) + len(expert_rows) + len(replay)
                consistency_losses = []
                for position, _group, count, weights, self_weight, terminal_value in search_groups:
                    parent = critic_values[len(rows) + position]
                    children = critic_values[child_offset:child_offset + count]
                    child_offset += count
                    expected = self_weight * parent + terminal_value
                    if count:
                        expected = expected + (children * torch.as_tensor(
                            weights, device=target
                        )).sum()
                    consistency_losses.append((parent - expected).square())
                search_consistency_loss = torch.stack(consistency_losses).mean()
            loss = policy_loss + critic_loss - entropy_weight * entropy \
                + args.expert_weight * expert_loss \
                + args.search_consistency_weight * search_consistency_loss
            ppo_head_grad = expert_head_grad = expert_ppo_grad_cosine = None
            if expert_count and args.expert_weight:
                policy_parameters = tuple(model.policy.parameters())
                ppo_gradients = torch.autograd.grad(
                    policy_loss, policy_parameters, retain_graph=True, allow_unused=True
                )
                expert_gradients = torch.autograd.grad(
                    args.expert_weight * expert_loss, policy_parameters,
                    retain_graph=True, allow_unused=True,
                )
                pairs = [(left.float(), right.float()) for left, right in zip(
                    ppo_gradients, expert_gradients
                ) if left is not None and right is not None]
                ppo_head_grad = torch.stack([left.square().sum() for left, _ in pairs]).sum().sqrt()
                expert_head_grad = torch.stack([
                    right.square().sum() for _, right in pairs
                ]).sum().sqrt()
                expert_ppo_grad_cosine = torch.stack([
                    (left * right).sum() for left, right in pairs
                ]).sum() / (ppo_head_grad * expert_head_grad).clamp_min(1e-12)
            replay_valid = replay_eligible = eligible_cpu = valid_cpu = None
            if replay:
                replay_start = len(rows) + len(expert_rows)
                replay_logits = all_logits[replay_start:]
                replay_legal = inputs[6][replay_start:]
                replay_masked = replay_logits.masked_fill(~replay_legal, -torch.inf)
                replay_action = torch.as_tensor([sample[1] for sample in replay], device=target)
                reference = np.full(replay_masked.shape, -np.inf, np.float32)
                for row, sample in zip(reference, replay):
                    row[:len(sample[4])] = sample[4]
                reference = torch.as_tensor(reference, device=target)
                replay_distribution = torch.distributions.Categorical(logits=replay_masked)
                reference_distribution = torch.distributions.Categorical(logits=reference)
                replay_kl = torch.distributions.kl_divergence(reference_distribution, replay_distribution)
                replay_capacity = fresh_count.to(torch.int64) // 9
                replay_eligible = torch.arange(len(replay), device=target) < replay_capacity
                replay_valid = replay_eligible & (replay_kl <= args.target_kl)
                eligible_cpu = replay_eligible.cpu().numpy()
                valid_cpu = replay_valid.cpu().numpy()
                replay_rejected = [sample for sample, eligible, keep in zip(
                    replay, eligible_cpu, valid_cpu,
                ) if eligible and not keep]
                reservoir.evict(replay_rejected)
                winning_rejected += len(replay_rejected)
                for sample in replay_rejected:
                    winning_evicted_characters[packed_character(sample[0])] += 1
                replay_weight = replay_valid.to(replay_kl.dtype)
                replay_denominator = replay_weight.sum().clamp_min(1)
                replay_log_probability = replay_distribution.log_prob(replay_action)
                reference_log_probability = reference_distribution.log_prob(replay_action)
                replay_log_ratio = torch.where(
                    replay_valid, replay_log_probability - reference_log_probability,
                    torch.zeros_like(replay_log_probability),
                )
                replay_ratio = replay_log_ratio.exp()
                replay_advantage = torch.as_tensor(
                    [sample[3] for sample in replay], device=target
                )
                winning_loss = -args.winning_loss_weight * (torch.minimum(
                    replay_ratio * replay_advantage,
                    replay_ratio.clamp(1 - args.clip, 1 + args.clip) * replay_advantage,
                ) * replay_weight).sum() / replay_capacity.clamp_min(1)
                loss = loss + winning_loss
                replay_kl_mean = (replay_kl * replay_weight).sum() / replay_denominator
            backward_started = time.monotonic()
            optimizer.zero_grad(set_to_none=True); loss.backward()
            if args.critic_only:
                gradient_norm = critic_only_step(model, optimizer); critic_only_updates += 1
                accepted, proposals, post_log_ratio = True, [0.], log_ratio.detach()
            elif args.disable_post_kl_check or args.target_kl >= 1:
                gradient_norm = float(nn.utils.clip_grad_norm_(model.parameters(), .5))
                optimizer.step()
                accepted, proposals, post_log_ratio = True, [], log_ratio.detach()
            else:
                accepted, proposals, post_log_ratio, gradient_norm = trust_region_step(
                    model, optimizer, inputs, action, old, fresh, denominator,
                    args.precision, args.target_kl, args.policy_temperature,
                    choice_index=choice_index if flat_policy else None,
                )
            rejected_proposals = sum(
                not math.isfinite(value) or value > args.target_kl for value in proposals
            )
            post_kl_checks += len(proposals)
            post_kl_proposals += len(proposals)
            post_kl_rejected_proposals += rejected_proposals
            post_kl_retries += max(0, len(proposals) - 1)
            post_kl_retry_depth = max(post_kl_retry_depth, max(0, len(proposals) - 1))
            if not accepted:
                handled += dataset.discard_trajectories(values["trajectory"][fresh_cpu])
                dataset.post_kl_dropped += fresh_rows
                post_kl_discarded_updates += 1
                optimizer.zero_grad(set_to_none=True)
                retry_logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[1][:len(rows)][fresh]
                retry_prediction = critic_value(retry_logits)
                retry_squared_error = (retry_prediction - batch_target).square()
                retry_value_loss = (retry_squared_error * critic_weights).mean()
                retry_explained_reward_variance, \
                    retry_floor_conditioned_explained_reward_variance, \
                    retry_explained_reward_variance_by_floor = \
                    critic_explained_reward_variance(
                        retry_prediction.detach(), batch_target,
                        values["canonical"][fresh_cpu],
                    )
                retry_critic_loss = args.value_weight * retry_value_loss
                retry_critic_loss.backward(inputs=critic_parameters)
                gradient_norm = critic_only_step(model, optimizer, critic_parameters)
                revisions["weights_revision"] += 1
                if (target.type == "mps" and args.mps_empty_cache_updates
                        and revisions["weights_revision"] % args.mps_empty_cache_updates == 0):
                    torch.mps.empty_cache()
                critic_only_updates += 1
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                backward_seconds = time.monotonic() - backward_started
                backward_durations.append(backward_seconds)
                losses["critic_explained_reward_variance"].append(
                    retry_explained_reward_variance
                )
                record_floor_explained(retry_explained_reward_variance_by_floor)
                training_batch_event(
                    "post_kl_rejected", "critic_only", attempted_rows=len(rows),
                    fresh_rows=fresh_rows, policy_trained_rows=0,
                    critic_trained_rows=fresh_rows, ratio_rejected_rows=ratio_rejected,
                    retired_rows=len(expired), critic_loss=float(retry_value_loss.detach()),
                    critic_explained_reward_variance=float(retry_explained_reward_variance),
                    critic_floor_conditioned_explained_reward_variance=
                    retry_floor_conditioned_explained_reward_variance,
                    critic_explained_reward_variance_by_floor=
                    retry_explained_reward_variance_by_floor,
                    advantage_mean=float(np.mean(values["advantage"][fresh_cpu])),
                    advantage_stddev=float(np.std(values["advantage"][fresh_cpu])),
                    policy_lag_mean=float(np.mean(batch_policy_lags)),
                    policy_lag_p95=float(np.quantile(batch_policy_lags, .95)),
                    policy_lag_max=max(batch_policy_lags),
                    entropy=float(entropy.detach()), entropy_weight=entropy_weight,
                    pre_kl=kl_value, post_kl_proposals=proposals,
                    clip_fraction=float(clip_fraction), gradient_norm=gradient_norm,
                    gradient_clipped=gradient_norm > .5, unpack_seconds=unpack_seconds,
                    forward_seconds=forward_seconds, backward_seconds=backward_seconds,
                    total_seconds=update_elapsed,
                )
                publish()
                continue
            post_kl = proposals[-1] if proposals else kl_value
            post_ratio = post_log_ratio.exp()
            observed_kl = post_kl
            observed_clip = float(
                (((post_ratio - 1).abs() > args.clip) * fresh).sum() / denominator
            )
            losses["post_kl"].append(post_kl)
            accepted_pre_kl_sum += kl_value
            accepted_post_kl_sum += post_kl
            losses["mean_advantage"].append(float(np.mean(values["advantage"][fresh_cpu])))
            losses["policy_loss"].append(policy_loss.detach())
            if expert_count and args.expert_weight:
                losses["expert_loss"].append(expert_loss.detach())
                losses["expert_entropy"].append(expert_entropy.detach())
                losses["expert_kl"].append((expert_loss - expert_entropy).detach())
                losses["expert_rows"].append(expert_count)
                losses["ppo_policy_head_grad_norm"].append(ppo_head_grad.detach())
                losses["expert_policy_head_grad_norm"].append(expert_head_grad.detach())
                losses["expert_ppo_grad_ratio"].append(
                    (expert_head_grad / ppo_head_grad.clamp_min(1e-12)).detach()
                )
                losses["expert_ppo_grad_cosine"].append(expert_ppo_grad_cosine.detach())
            losses["critic_loss"].append(value_loss.detach())
            losses["critic_explained_reward_variance"].append(explained_reward_variance)
            record_floor_explained(explained_reward_variance_by_floor)
            if search_groups:
                losses["search_consistency_loss"].append(search_consistency_loss.detach())
            base_prediction = prediction.detach() + torch.as_tensor(
                values["potential"][fresh_cpu], device=target
            )
            losses["critic_expected"].append(base_prediction.mean())
            losses["critic_win_probability"].append(
                critic_win_probability(base_prediction).mean()
            )
            losses["entropy"].append(entropy.detach())
            losses["entropy_weight"].append(entropy_weight)
            losses["kl"].append(kl); losses["clip_fraction"].append(clip_fraction)
            trained += fresh_rows; updates += 1
            revisions["weights_revision"] += 1
            if not args.critic_only:
                revisions["policy_revision"] += 1
            if expert_count:
                expert_visits += int(expert_visits_batch.sum())
                expert_depth += int(expert_depths_batch.sum())
                expert_dataset.used += expert_count
                expert_dataset.discard_ids(expert_ids)
            if replay_valid is not None:
                replay_characters = [packed_character(sample[0]) for sample in replay]
                for character, eligible, keep in zip(
                    replay_characters, eligible_cpu, valid_cpu
                ):
                    if eligible and keep:
                        winning_replayed_characters[character] += 1
                replay_count = int(valid_cpu.sum())
                if replay_count:
                    behavior = torch.as_tensor([sample[2] for sample in replay], device=target)
                    winning_behavior_drift = (
                        replay_log_probability.detach()[replay_valid] - behavior[replay_valid]
                    ).abs().mean()
                    losses["winning_loss"].append(winning_loss.detach())
                    losses["winning_kl"].append(replay_kl_mean.detach())
                    winning_replayed += replay_count
            if (target.type == "mps" and args.mps_empty_cache_updates
                    and revisions["weights_revision"] % args.mps_empty_cache_updates == 0):
                torch.mps.empty_cache()
            backward_seconds = time.monotonic() - backward_started
            backward_durations.append(backward_seconds)
            update_elapsed = time.monotonic() - update_started
            update_seconds += update_elapsed
            update_durations.append(update_elapsed)
            training_batch_event(
                "disabled" if args.critic_only else "accepted",
                "critic_only" if args.critic_only else "full",
                attempted_rows=len(rows), fresh_rows=fresh_rows,
                policy_trained_rows=0 if args.critic_only else fresh_rows,
                critic_trained_rows=fresh_rows, ratio_rejected_rows=ratio_rejected,
                retired_rows=len(expired), local_policy_updates=updates,
                expert_rows=expert_count, replay_rows=len(replay),
                policy_loss=float(policy_loss.detach()),
                expert_loss=float(expert_loss.detach()),
                ppo_head_grad=float(ppo_head_grad.detach()) if ppo_head_grad is not None else 0,
                expert_head_grad=float(expert_head_grad.detach())
                if expert_head_grad is not None else 0,
                expert_grad_cosine=float(expert_ppo_grad_cosine.detach())
                if expert_ppo_grad_cosine is not None else 0,
                critic_loss=float(value_loss.detach()),
                critic_explained_reward_variance=float(explained_reward_variance),
                critic_floor_conditioned_explained_reward_variance=
                floor_conditioned_explained_reward_variance,
                critic_explained_reward_variance_by_floor=explained_reward_variance_by_floor,
                search_consistency_loss=float(search_consistency_loss.detach()),
                critic_expected=float(base_prediction.mean()),
                critic_win_probability=float(critic_win_probability(base_prediction).mean()),
                advantage_mean=float(np.mean(values["advantage"][fresh_cpu])),
                advantage_stddev=float(np.std(values["advantage"][fresh_cpu])),
                policy_lag_mean=float(np.mean(batch_policy_lags)),
                policy_lag_p95=float(np.quantile(batch_policy_lags, .95)),
                policy_lag_max=max(batch_policy_lags),
                entropy=float(entropy.detach()), entropy_weight=entropy_weight,
                pre_kl=kl_value, post_kl=post_kl,
                post_kl_proposals=proposals, clip_fraction=float(clip_fraction),
                gradient_norm=gradient_norm, gradient_clipped=gradient_norm > .5,
                learning_rates=[group["lr"] for group in optimizer.param_groups],
                unpack_seconds=unpack_seconds, forward_seconds=forward_seconds,
                backward_seconds=backward_seconds, total_seconds=update_elapsed,
            )
            if updates % args.publish_updates == 0:
                publish()
            if update_elapsed > 5:
                packed = rows + expert_rows + [sample[0] for sample in replay]
                represented_actions = max(packed_action_count(row) for row in packed)
                emit_event({"event": "slow_update", "metrics": {
                    "seconds": update_elapsed,
                    "fresh": fresh_rows, "replay": len(replay),
                    "state_tokens_max": max(packed_state_count(row) for row in packed),
                    "represented_actions_max": represented_actions,
                    "state_attention_pairs": sum((packed_state_count(row) + 1) ** 2 for row in packed),
                    "legal_actions_max": max(packed_legal_count(row) for row in packed),
                    "unpack_seconds": unpack_seconds,
                    "forward_seconds": forward_seconds, "backward_seconds": backward_seconds,
                }}, "WARNING")
            absolute = base_decisions + handled
            if absolute >= next_save:
                save_step(absolute)
                next_save += args.save_decisions
            if absolute >= next_report:
                windows += 1
                report()
                next_report += args.report_decisions
                update_promotion()
                if promotion_ready:
                    stop.set()
    finally:
        stop.set()
        if watchdog.ident is not None:
            watchdog.join()
        if receiver and receiver.ident is not None:
            receiver.join()
        recover_workers()
        started_workers = [worker for worker in workers if worker is not None and (threaded or worker.pid is not None)]
        shutdown_deadline = time.monotonic() + 5
        while any(worker.is_alive() for worker in started_workers) and time.monotonic() < shutdown_deadline:
            drain_results()
            for worker in started_workers:
                worker.join(.05)
        terminated = set() if threaded else {worker for worker in started_workers if worker.is_alive()}
        for worker in terminated:
            worker.terminate()
        for worker in terminated:
            worker.join(5)
        for worker in terminated:
            if worker.is_alive():
                worker.kill()
        for worker in terminated:
            worker.join()
        drain_results()
        for index, worker in enumerate(workers):
            observed = int(progress[index])
            sampled += max(0, observed - worker_accounted[index])
            dropped = max(0, observed - worker_resolved[index])
            discarded_steps += dropped
            watchdog_dropped += dropped * (worker in terminated)
        if not threaded:
            for queue in (*models, sample_source, results):
                queue.cancel_join_thread()
                queue.close()
        packer.shutdown(wait=True, cancel_futures=True)
    failed = [] if threaded else [worker.exitcode for worker in started_workers
                                  if worker not in terminated
                                  and worker not in watchdog_terminated and worker.exitcode]
    if failed:
        raise RuntimeError(f"sampler processes failed: {failed}")
    assert handled == decisions and not len(dataset)
    unresolved = sampled - decisions - discarded_steps
    assert unresolved >= 0, (sampled, decisions, discarded_steps)
    discarded_steps += unresolved
    if handled > reported_steps:
        windows += 1
        report()
    terminal = [episode for character in episodes for episode in character]
    terminal_summary = episode_summary(terminal)
    terminal_summary["empty_actions"] += orphan_empty_actions
    _, _, curriculum = curriculum_weights(
        stage, stage_decisions + decisions, auxiliary_decisions + decisions, args, progress_active
    )
    return {
        "decisions": decisions, "updates": updates, "windows": windows,
        "decisions_per_second": decisions / (time.monotonic() - started),
        "seconds": time.monotonic() - started,
        "collect_seconds": collect_seconds, "screen_seconds": screen_seconds,
        "screen_unpack_seconds": screen_unpack_seconds,
        "screen_forward_seconds": screen_forward_seconds,
        "update_seconds": update_seconds,
        "actor_decisions_per_second": sampled / max(1e-9, collect_seconds),
        "learner_decisions_per_second": trained / max(1e-9, update_seconds),
        "mcts_roots": mcts_roots, "mcts_simulations": mcts_simulations,
        "mcts_leaves": mcts_leaves, "mcts_nodes": mcts_nodes,
        "mcts_batches": mcts_batches, "mcts_targets": mcts_targets,
        "mcts_turn_starts": mcts_turn_starts,
        "mcts_seconds": mcts_seconds,
        "mcts_root_fraction": mcts_roots / max(1, mcts_turn_starts),
        "mcts_roots_per_decision": mcts_roots / max(1, sampled),
        "mcts_simulations_per_root": mcts_simulations / max(1, mcts_roots),
        "mcts_simulations_per_second": mcts_simulations / max(1e-9, mcts_seconds),
        "mcts_leaf_batch_mean": mcts_leaves / max(1, mcts_batches),
        "mcts_targets_per_root": mcts_targets / max(1, mcts_roots),
        "mcts_simulate_fraction": mcts_simulate_seconds / max(1e-9, mcts_seconds),
        "mcts_encode_fraction": mcts_encode_seconds / max(1e-9, mcts_seconds),
        "mcts_inference_fraction": mcts_inference_seconds / max(1e-9, mcts_seconds),
        "mcts_backup_fraction": mcts_backup_seconds / max(1e-9, mcts_seconds),
        "mcts_rollout_steps": mcts_rollout_steps,
        "mcts_rollout_completed": mcts_rollout_completed,
        "mcts_rollout_invalid": mcts_rollout_invalid,
        "mcts_timeouts": mcts_timeouts,
        "mcts_rollout_fraction": mcts_rollout_seconds / max(1e-9, mcts_seconds),
        "expert_buffer_rows": len(expert_dataset),
        "expert_rows_seen": expert_dataset.seen,
        "expert_rows_used": expert_dataset.used,
        "expert_rows_stale": expert_dataset.stale,
        "expert_rows_evicted": expert_dataset.evicted,
        "expert_visit_mean": expert_visits / max(1, expert_dataset.used),
        "expert_depth_mean": expert_depth / max(1, expert_dataset.used),
        "row_utilization": trained / max(1, attempted),
        "update_seconds_p95": float(np.quantile(update_durations, .95)) if update_durations else 0,
        "unpack_seconds_p95": float(np.quantile(unpack_durations, .95)) if unpack_durations else 0,
        "forward_seconds_p95": float(np.quantile(forward_durations, .95)) if forward_durations else 0,
        "backward_seconds_p95": float(np.quantile(backward_durations, .95)) if backward_durations else 0,
        **cache_stats,
        "card_cache_hit_rate": cache_stats["card_hit"] / max(
            1, cache_stats["card_hit"] + cache_stats["card_miss"]
        ),
        "graph_cache_hit_rate": cache_stats["graph_hit"] / max(
            1, cache_stats["graph_hit"] + cache_stats["graph_miss"]
        ),
        "stage": {"index": stage, "ascension": ascension, "bonus": bonus},
        "characters": summaries(episodes),
        "terminals": terminal_summary,
        "description": f"Continuously trained V{model.model_version} on A{ascension}/+{bonus} trajectories.",
        "pipeline": pipeline, "promotion_ready": promotion_ready,
        "promotion_result": promotion_result,
        "dataset_rows": len(dataset), "dataset_peak": dataset_peak,
        "dataset_seen": dataset.seen, "dataset_admitted": dataset.admitted,
        "dataset_uses": dataset.uses, "dataset_retired": dataset.retired,
        "dataset_forced_dropped": dataset.forced_dropped,
        "dataset_capacity_dropped": dataset.capacity_dropped,
        "dataset_incomplete_dropped": dataset.incomplete_dropped,
        "dataset_priority_mean": float(dataset.data["priority"][:len(dataset)].mean()) if len(dataset) else 0,
        "dataset_priority_max": float(dataset.data["priority"][:len(dataset)].max()) if len(dataset) else 0,
        "dataset_attempted": attempted, "dataset_trained": trained,
        "dataset_stale_dropped": dataset.stale_dropped,
        "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
        "dataset_post_kl_dropped": dataset.post_kl_dropped,
        "post_kl_checks": post_kl_checks,
        **optimizer_metrics(),
        "sampled_decisions": sampled, "discarded_steps": discarded_steps,
        "discarded_step_fraction": discarded_steps / max(1, sampled),
        "completed_trajectories": len(trajectory_lengths),
        "bootstrapped_segments": segmented_trajectories,
        "trajectory_length_mean": float(np.mean(trajectory_lengths)) if trajectory_lengths else 0,
        "trajectory_length_max": max(trajectory_lengths, default=0),
        "trajectory_policy_span_mean": float(np.mean(trajectory_policy_spans)) if trajectory_policy_spans else 0,
        "trajectory_policy_span_max": max(trajectory_policy_spans, default=0),
        "trajectory_arrival_lag_mean": float(np.mean(trajectory_arrival_lags)) if trajectory_arrival_lags else 0,
        "trajectory_arrival_lag_max": max(trajectory_arrival_lags, default=0),
        "trajectory_stale_step_fraction": sum(trajectory_stale_steps) / max(1, sum(trajectory_lengths)),
        "trajectory_completion_seconds_mean": float(np.mean(trajectory_seconds)) if trajectory_seconds else 0,
        "trajectory_completion_seconds_max": max(trajectory_seconds, default=0),
        "policy_version": revisions["policy_revision"],
        "weights_revision": revisions["weights_revision"],
        "actor_revision": revisions["actor_revision"],
        "sampler_version": latest_sampler_version,
        "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
        "sampler_heartbeats": [time.monotonic() - value for value in heartbeat[:]],
        "sampler_restarts": sampler_restarts, "sampler_wedges": sampler_wedges,
        "sampler_restart_streaks": sampler_restart_streaks,
        "watchdog_dropped_steps": watchdog_dropped,
        "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
        "sample_queue_packets": queue_packets,
        "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
        "sample_queue_full_waits": queue_full_waits,
        "sample_queue_put_seconds": queue_put_seconds,
        "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
        "winning_character_rows": list(map(len, reservoir.by_character())),
        "winning_episodes": reservoir.wins, "winning_added": winning_added,
        "winning_skipped": reservoir.skipped, "winning_forced_skipped": reservoir.forced,
        "winning_replayed": winning_replayed,
        "winning_rejected_kl": winning_rejected,
        "winning_replayed_characters": winning_replayed_characters,
        "winning_evicted_characters": winning_evicted_characters,
        "winning_replay_fraction": winning_replayed / max(1, trained + winning_replayed),
        "winning_loss_weight": args.winning_loss_weight,
        "winning_collection_drift": float(winning_behavior_drift),
        "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
        "progress_active": progress_active, "curriculum": curriculum,
        **critic_balance.report(),
        **metric_means(losses),
    }


def evaluate(model, args, target, seed=None, runs=None, stage=None, max_steps=None, max_combat_steps=None):
    was_training = model.training
    model.eval()
    ascension, bonus = STAGES[stage] if stage is not None else (args.ascension, args.bonus)
    reports = []
    for character in range(5):
        outcomes = []; floors = []
        caps = step_caps = combat_caps = empty_count = 0
        total = runs or args.runs
        for start in range(0, total, args.evaluation_batch):
            count = min(args.evaluation_batch, total - start)
            base_seed = args.validation_seed if seed is None else seed
            env = sts2_sim.Batch(
                count, base_seed + character * 1_000_000 + start,
                character, ascension=ascension,
            )
            env.set_training_bonus(bonus)
            active = np.ones(count, bool)
            capped = np.zeros(count, bool)
            empty_actions = np.zeros(count, bool)
            step_capped = np.zeros(count, bool)
            combat_capped = np.zeros(count, bool)
            steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            for _ in range(max_steps or args.max_steps):
                if not active.any():
                    break
                observation = env.observe_tokens(active.tolist())
                empty = active & ~observation_legal(observation).any(1)
                empty_actions |= empty
                capped |= empty
                active &= ~empty
                if not active.any():
                    break
                indices = np.flatnonzero(active)
                rows = pack_batch(observation, False)
                if len(rows) != count:
                    raise ValueError("packed observation row count mismatch")
                inputs = unpack([rows[index] for index in indices], target, model, False)
                with torch.inference_mode():
                    logits = predict(model, inputs, args.precision, policy_only=True)
                    selected = logits.masked_fill(~inputs[6], -torch.inf).argmax(1).cpu().numpy()
                choice = np.zeros(count, np.int64); choice[indices] = selected
                in_combat = np.asarray([row[4] == 1 for row in env.stats()])
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                _, done, _ = env.step(choice.tolist(), active.tolist())
                done = np.asarray(done, bool)
                steps += active
                still_combat = np.asarray([row[4] == 1 for row in env.stats()])
                cap = active & ~done & (
                    (steps >= (max_steps or args.max_steps))
                    | ((combat_steps >= (max_combat_steps or args.max_combat_steps)) & still_combat)
                )
                step_capped |= cap & (steps >= (max_steps or args.max_steps))
                combat_capped |= cap & ~step_capped
                capped |= cap
                active &= ~(done | cap)
            for index, row in enumerate(env.stats()):
                outcomes.append(int(not capped[index] and row[4] == 12))
                floors.append(int((row[0] - 1) * 17 + row[1]))
            caps += int(capped.sum())
            step_caps += int(step_capped.sum())
            combat_caps += int(combat_capped.sum())
            empty_count += int(empty_actions.sum())
        wins = sum(outcome == 1 for outcome in outcomes)
        boss_entries = sum(outcome == 1 or floor >= 51 for outcome, floor in zip(outcomes, floors))
        reports.append({
            "character": character, "runs": total, "resolved": total - caps, "wins": wins,
            "win_rate": wins / total, "win_rate_interval": wilson(wins, total),
            "floor_mean": float(np.mean(floors)),
            "floor_bands": bands(floors), "outcomes": outcomes, "floors": floors,
            "boss_entries": boss_entries, "boss_entry_rate": boss_entries / total,
            "boss_entry_rate_interval": wilson(boss_entries, total),
            "boss_conversion": wins / max(1, boss_entries),
            "boss_conversion_interval": wilson(wins, boss_entries),
            "caps": caps, "step_caps": step_caps, "combat_caps": combat_caps,
            "empty_actions": empty_count,
        })
    if was_training:
        model.train()
    wins = sum(row["wins"] for row in reports)
    total = sum(row["runs"] for row in reports)
    boss_entries = sum(row["boss_entries"] for row in reports)
    return {
        "stage": {"ascension": ascension, "bonus": bonus}, "characters": reports,
        "wins": wins, "runs": total, "win_rate": wins / total,
        "win_rate_interval": wilson(wins, total),
        "floor_mean": float(np.mean([floor for row in reports for floor in row["floors"]])),
        "boss_entries": boss_entries, "boss_entry_rate": boss_entries / total,
        "boss_entry_rate_interval": wilson(boss_entries, total),
        "boss_conversion": wins / max(1, boss_entries),
        "boss_conversion_interval": wilson(wins, boss_entries),
        "caps": sum(row["caps"] for row in reports),
    }


def evaluate_search_policies(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    policy = export_value_model(None, model, checkpoint["fingerprint"], 1, 0, True)
    modes = []
    for mode in args.modes:
        started = time.monotonic(); characters = []
        totals = dict.fromkeys((
            "mcts_roots", "mcts_simulations", "mcts_targets", "mcts_rollout_steps",
            "mcts_rollout_completed", "mcts_rollout_invalid", "mcts_rollout_micros",
            "search_hits", "search_misses",
            "exact_calls", "exact_states", "exact_transitions", "exact_rng_transitions",
            "exact_rng_calls", "exact_failures",
        ), 0)
        for character in args.characters:
            outcomes = []; floors = []; caps = 0
            for start in range(0, args.runs, args.batch):
                count = min(args.batch, args.runs - start)
                env = sts2_sim.Batch(
                    count, args.seed + character * 1_000_000 + start,
                    character, ascension=ascension,
                )
                env.set_training_bonus(bonus); env.load_policy(policy)
                active = np.ones(count, bool); steps = np.zeros(count, np.int32)
                combat_steps = np.zeros(count, np.int32); turn_depth = np.zeros(count, np.int32)
                previous_turn = np.full(count, -1, np.int32); targets = {}
                for _ in range(args.max_steps):
                    if not active.any():
                        break
                    stats = env.stats()
                    in_combat = np.asarray([row[4] == 1 for row in stats])
                    turns = np.asarray([row[5] if row[4] == 1 else -1 for row in stats])
                    fresh_turn = active & in_combat & (turns != previous_turn)
                    turn_depth[fresh_turn] = 0
                    result = env.policy(
                        args.policy_temperature, False, False,
                        mcts_fraction=1 if mode not in ("policy", "exact") else 0,
                        mcts_simulations=args.simulations if mode not in ("policy", "exact") else 0,
                        mcts_boss_simulations=args.simulations
                        if mode not in ("policy", "exact") else 0,
                        mcts_turns=0 if mode.startswith("combat") else 1,
                        mcts_max_depth=args.max_depth,
                        mcts_batch_size=args.search_batch, mcts_min_visits=args.min_visits,
                        mcts_max_targets=args.max_targets,
                        mcts_prior_temperature=args.prior_temperature,
                        mcts_q_temperature=args.q_temperature,
                        mcts_exploration=args.exploration,
                        mcts_heuristic=mode.endswith("heuristic"),
                    )
                    choices = np.asarray(result[1], np.int64); rows = result[4]
                    if mode != "policy" and mode != "exact":
                        search = np.asarray(result[6], np.int64)
                        totals["mcts_roots"] += int(search[0])
                        totals["mcts_simulations"] += int(search[1])
                        totals["mcts_targets"] += int(search[5])
                        totals["mcts_rollout_steps"] += int(search[12])
                        totals["mcts_rollout_completed"] += int(search[13])
                        totals["mcts_rollout_invalid"] += int(search[14])
                        totals["mcts_rollout_micros"] += int(search[15])
                        for row, target, _visits, depth, *_ in result[5]:
                            targets[bytes(row), int(depth)] = int(np.argmax(target))
                    elif mode == "exact":
                        indices = [int(index) for index in np.flatnonzero(fresh_turn)
                                   if packed_legal_count(rows[index]) > 1]
                        exact = env.exact_choices(
                            indices, 1, args.max_depth, args.exact_samples,
                            args.max_exact_states,
                            args.seed + character * 1_000_000 + start, True,
                        )
                        for index, row in zip(indices, exact):
                            (_choice, _value, states, transitions, rng_transitions,
                             rng, plan, error) = row
                            totals["exact_calls"] += 1
                            totals["exact_states"] += states
                            totals["exact_transitions"] += transitions
                            totals["exact_rng_transitions"] += rng_transitions
                            totals["exact_rng_calls"] += rng
                            if error:
                                totals["exact_failures"] += 1
                            else:
                                for packed, choice, depth in plan:
                                    targets[bytes(packed), int(depth)] = choice
                    if mode != "policy":
                        for index in np.flatnonzero(active & in_combat):
                            choice = targets.get((bytes(rows[index]), int(turn_depth[index])))
                            if choice is None:
                                totals["search_misses"] += packed_legal_count(rows[index]) > 1
                            else:
                                choices[index] = choice; totals["search_hits"] += 1
                    combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                    turn_depth[active & in_combat] += 1
                    previous_turn = np.where(active & in_combat, turns, -1)
                    _, done, _ = env.step(choices.tolist(), active.tolist())
                    done = np.asarray(done, bool); steps += active
                    final = env.stats()
                    still_combat = np.asarray([row[4] == 1 for row in final])
                    capped = active & ~done & (
                        (steps >= args.max_steps)
                        | ((combat_steps >= args.max_combat_steps) & still_combat)
                    )
                    for index in np.flatnonzero(done | capped):
                        outcomes.append(int(done[index] and final[index][4] == 12))
                        floors.append(int((final[index][0] - 1) * 17 + final[index][1]))
                    caps += int(capped.sum())
                    finished = np.flatnonzero(done | capped).tolist()
                    active &= ~(done | capped)
                    if finished:
                        env.reset(finished, 0)
            wins = sum(outcomes); boss_entries = sum(
                won or floor >= 51 for won, floor in zip(outcomes, floors)
            )
            characters.append({
                "character": character, "runs": args.runs, "wins": wins,
                "win_rate": wins / args.runs, "floor_mean": float(np.mean(floors)),
                "boss_entries": boss_entries, "boss_entry_rate": boss_entries / args.runs,
                "boss_conversion": wins / max(1, boss_entries), "caps": caps,
                "outcomes": outcomes, "floors": floors,
            })
            print(json.dumps({"mode": mode, "character": character, "wins": wins,
                              "floor_mean": characters[-1]["floor_mean"],
                              "seconds": time.monotonic() - started}), flush=True)
        wins = sum(row["wins"] for row in characters); runs = len(args.characters) * args.runs
        boss_entries = sum(row["boss_entries"] for row in characters)
        modes.append({
            "mode": mode, "runs": runs, "wins": wins, "win_rate": wins / runs,
            "floor_mean": float(np.mean([floor for row in characters for floor in row["floors"]])),
            "boss_entries": boss_entries, "boss_entry_rate": boss_entries / runs,
            "boss_conversion": wins / max(1, boss_entries), "caps": sum(row["caps"] for row in characters),
            "seconds": time.monotonic() - started, "characters": characters, **totals,
        })
        print(json.dumps({key: modes[-1][key] for key in (
            "mode", "runs", "wins", "win_rate", "floor_mean", "boss_entries",
            "boss_conversion", "caps", "seconds", "search_hits", "search_misses",
            "mcts_simulations", "mcts_rollout_steps", "mcts_rollout_completed",
            "mcts_rollout_invalid", "mcts_rollout_micros", "exact_calls", "exact_states",
            "exact_failures",
        )}), flush=True)
    report = {
        "checkpoint": args.checkpoint, "step": checkpoint["decisions"],
        "stage": {"index": checkpoint["stage"], "ascension": ascension, "bonus": bonus},
        "settings": {key: value for key, value in vars(args).items() if key != "command"},
        "modes": modes,
    }
    immutable_json(Path(args.output), report)


def critic_metrics(rows):
    result = {"states": len(rows)}
    for name, target_key, prediction_key, scale in (
        ("win", "win", "win_prediction", 1),
        ("floor", "terminal_floor", "floor_prediction", CATEGORIES - 1),
    ):
        target = np.asarray([row[target_key] for row in rows], np.float32)
        prediction = np.asarray([row[prediction_key] for row in rows], np.float32)
        error = (prediction - target) * scale
        correlation = float(np.corrcoef(target, prediction)[0, 1]) \
            if len(rows) > 1 and target.std() > 1e-7 and prediction.std() > 1e-7 else None
        correlation = correlation if correlation is None or math.isfinite(correlation) else None
        result[name] = {
            "target_mean": float(target.mean()), "prediction_mean": float(prediction.mean()),
            "target_std": float(target.std()), "prediction_std": float(prediction.std()),
            "bias": float(error.mean()), "mae": float(np.abs(error).mean()),
            "rmse": float(np.sqrt(np.mean(error ** 2))), "correlation": correlation,
        }
    return result


def critic_diagnostics(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    policy = export_value_model(None, model, checkpoint["fingerprint"], 1, 0, True)
    states = []
    transitions = []
    capped_episodes = 0
    for character in range(5):
        for start in range(0, args.runs, args.batch):
            count = min(args.batch, args.runs - start)
            env = sts2_sim.Batch(
                count, args.seed + character * 1_000_000 + start,
                character, ascension=ascension,
            )
            env.set_training_bonus(bonus)
            env.set_potential_weights(model.potential_weights)
            env.load_policy(policy)
            active = np.ones(count, bool)
            histories = [[] for _ in range(count)]
            steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            for _ in range(args.max_steps):
                if not active.any():
                    break
                stats = env.stats()
                result = env.policy(args.policy_temperature, True, True)
                shaped = np.asarray(result[3], np.float32)
                floor = shaped + np.asarray(result[14], np.float32)
                win = critic_win_probability(torch.from_numpy(floor)).numpy()
                for index in np.flatnonzero(active):
                    row = stats[index]
                    histories[index].append({
                        "character": character, "phase": int(row[4]),
                        "floor": int(row[9]),
                        "reward_screen": row[4] == 2,
                        "win_prediction": float(win[index]),
                        "floor_prediction": float(floor[index]),
                        "critic_value": float(shaped[index]),
                    })
                done = np.asarray(result[8], bool)
                final = result[9]
                legal = np.asarray(result[10], bool)
                in_combat = np.asarray(result[11], bool)
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                steps += active
                still_combat = np.asarray([row[4] == 1 for row in final])
                capped = active & ~done & (
                    (steps >= args.max_steps)
                    | ((combat_steps >= args.max_combat_steps) & still_combat)
                    | ~legal
                )
                for index in np.flatnonzero(active & done):
                    terminal_category = CATEGORIES - 1 if final[index][4] == 12 else int(final[index][9])
                    path = histories[index]
                    successor = [state["floor_prediction"] for state in path[1:]] \
                        + [terminal_category / (CATEGORIES - 1)]
                    transitions.extend({
                        "floor": state["floor"],
                        "residual": (state["floor_prediction"] - next_value) * (CATEGORIES - 1),
                    } for state, next_value in zip(path, successor))
                    states.extend(state | {
                        "win": int(final[index][4] == 12),
                        "terminal_floor": terminal_category / (CATEGORIES - 1),
                        "terminal_category": terminal_category,
                    } for state in histories[index])
                capped_episodes += int(capped.sum())
                finished = np.flatnonzero(done | capped | ~legal).tolist()
                active &= ~(done | capped)
                if finished:
                    env.reset(finished, 0)
    phase_names = ("map", "combat", "rewards", "shop", "rest", "event", "remove",
                   "upgrade", "transform", "enchant", "choose_cards", "choose_bundles")
    grouped = {}
    for name, key in (
        ("character", lambda row: row["character"]),
        ("phase", lambda row: phase_names[row["phase"]]),
        ("floor", lambda row: row["floor"]),
        ("reward_screen", lambda row: row["reward_screen"]),
    ):
        values = {}
        for row in states:
            values.setdefault(str(key(row)), []).append(row)
        grouped[name] = {value: critic_metrics(rows) for value, rows in values.items()}
    residual = np.asarray([row["residual"] for row in transitions], np.float32)
    transition_consistency = {"transitions": len(residual)}
    if len(residual):
        transition_consistency |= {
            "bias": float(residual.mean()), "mae": float(np.abs(residual).mean()),
            "rmse": float(np.sqrt(np.mean(residual ** 2))),
            "p95_absolute": float(np.quantile(np.abs(residual), .95)),
        }
    report = {
        "checkpoint": args.checkpoint, "step": checkpoint["decisions"],
        "stage": {"index": checkpoint["stage"], "ascension": ascension, "bonus": bonus},
        "runs_per_character": args.runs, "states": len(states),
        "capped_episodes": capped_episodes, "overall": critic_metrics(states),
        "groups": grouped,
        "transition_consistency": transition_consistency,
    }
    immutable_json(Path(args.output), report)
    print(json.dumps({key: report[key] for key in (
        "checkpoint", "step", "stage", "runs_per_character", "states",
        "capped_episodes", "overall", "transition_consistency",
    )}, indent=2))


def load_stage_bests(output, development, checkpoints, best):
    stages = {
        key: row for key, row in best.get("stages", {}).items()
        if (output / row["checkpoint"]).exists()
    }
    for checkpoint in checkpoints.glob("*.pt"):
        try:
            step = int(checkpoint.stem.split("-w", 1)[0])
        except ValueError:
            continue
        development_path = development / f"{step:012}.json"
        if not development_path.exists():
            continue
        evaluation = json.loads(development_path.read_text())
        key = str(evaluation["stage"]["index"])
        record = {
            "step": evaluation["step"], "stage": evaluation["stage"],
            "score": evaluation["score"], "development": evaluation["result"],
            "checkpoint": str(checkpoint.relative_to(output)),
        }
        if tuple(record["score"]) > tuple(stages.get(key, {}).get("score", ())):
            record["sha256"] = (best["sha256"]
                                if best.get("checkpoint") == record["checkpoint"] and best.get("sha256")
                                else sha256_file(checkpoint))
            stages[key] = record
    return stages


def optimizer_groups(model, policy_multiplier, critic_multiplier):
    parameters = list(model.parameters())
    policy = {id(parameter) for parameter in model.policy.parameters()}
    critic = {id(parameter) for parameter in model.critic.parameters()}
    indices = [
        [index for index, parameter in enumerate(parameters)
         if id(parameter) not in policy | critic],
        [index for index, parameter in enumerate(parameters) if id(parameter) in policy],
        [index for index, parameter in enumerate(parameters) if id(parameter) in critic],
    ]
    return [
        {"params": [parameters[index] for index in group], "lr_scale": scale}
        for group, scale in zip(indices, (1., policy_multiplier, critic_multiplier))
    ], indices


def repartition_optimizer(state, index_groups, selected=None):
    groups = state["param_groups"]
    parameter_count = sum(map(len, index_groups))
    saved = [parameter for group in groups for parameter in group["params"]]
    if len(saved) != parameter_count:
        return False
    if len(groups) == 1:
        by_index = saved
        templates = groups * len(index_groups)
    elif len(groups) == len(index_groups) and all(
        len(group["params"]) == len(indices) for group, indices in zip(groups, index_groups)
    ):
        by_index = [None] * parameter_count
        for group, indices in zip(groups, index_groups):
            for index, parameter in zip(indices, group["params"]):
                by_index[index] = parameter
        templates = groups
    else:
        return False
    groups = [
        copy.deepcopy(template) | {
            "params": [by_index[index] for index in indices], "lr_scale": scale,
        }
        for template, indices in zip(templates, index_groups)
        for scale in [template.get("lr_scale", 1.)]
    ]
    if selected is not None:
        groups = [groups[selected]]
        parameters = set(groups[0]["params"])
        state["state"] = {key: value for key, value in state["state"].items()
                          if key in parameters}
    state["param_groups"] = groups
    return True


def train(args):
    if args.expert_batch is None:
        args.expert_batch = args.batch
    qos = getattr(ctypes.CDLL(None), "pthread_set_qos_class_self_np", None)
    if qos is not None:
        qos(0x21, 0)
    torch.set_num_threads(args.torch_threads)
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    target = device()
    if not 0 <= args.start_stage < len(STAGES):
        raise ValueError("invalid start stage")
    if args.precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    if args.precision != "fp32" and target.type not in ("mps", "cuda"):
        raise ValueError(f"{args.precision} requires MPS or CUDA")
    if min(args.envs, args.samplers, args.torch_threads, args.sampler_threads,
           args.batch, args.sampler_steps,
           args.publish_updates,
           args.report_decisions,
           args.save_decisions, args.promotion_window, args.promotion_runs,
           args.development_runs, args.progress_decisions, args.dataset_capacity,
           args.max_policy_lag + 1) < 1 or min(args.max_log_ratio, args.policy_temperature) <= 0:
        raise ValueError("invalid asynchronous replay settings")
    if args.dataset_capacity < args.batch:
        raise ValueError("dataset capacity must cover one batch")
    if args.sampler_timeout <= 0 or args.sampler_restarts < 0:
        raise ValueError("invalid sampler watchdog")
    if args.mps_empty_cache_updates < 0 or args.learning_rate_warmup_steps < 0:
        raise ValueError("invalid optimizer interval")
    if (args.segment_steps < 0 or args.winning_capacity < 0 or args.priority_decay <= 0
            or min(args.critic_consistency_weight, args.search_consistency_weight) < 0
            or min(args.critic_consistency_batch, args.search_consistency_batch) < 1):
        raise ValueError("invalid replay setting")
    if min(args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier) <= 0:
        raise ValueError("invalid head learning-rate multiplier")
    if not all(math.isfinite(getattr(args, f"potential_{term}_weight"))
               for term in POTENTIAL_TERMS):
        raise ValueError("invalid potential weights")
    if (args.segment_steps or not 0 <= args.gae_gamma <= 1
            or not 0 <= args.gae_lambda <= 1
            or not 0 <= args.critic_balance_decay < 1
            or args.blended_critic or args.critic_consistency_weight
            or args.search_consistency_weight and not args.critic_only):
        raise ValueError("invalid critic settings")
    if args.entropy_weight is not None and args.entropy_weight < 0:
        raise ValueError("invalid entropy weight")
    if (not 0 <= args.mcts_fraction <= 1
            or min(args.mcts_simulations, args.mcts_boss_simulations, args.mcts_turns,
                   args.expert_max_lag) < 0
            or min(args.mcts_max_depth, args.mcts_batch_size, args.mcts_min_visits,
                   args.mcts_max_targets, args.expert_batch, args.expert_capacity) < 1
            or min(args.mcts_prior_temperature, args.mcts_q_temperature) <= 0
            or args.mcts_exploration < 0 or args.expert_weight < 0
            or not math.isfinite(args.mcts_timeout) or args.mcts_timeout < 0):
        raise ValueError("invalid MCTS settings")
    if args.envs % args.samplers:
        raise ValueError("environments must be divisible by samplers")
    if not math.isfinite(args.hours) or args.hours < 0 or args.decisions < 0:
        raise ValueError("invalid training limit")
    if args.critic_only and not args.checkpoint:
        raise ValueError("critic-only training requires --checkpoint")
    if not 0 <= args.promotion_trigger_rate <= args.promote_win_rate <= 1:
        raise ValueError("invalid promotion rates")
    if seed_panel(args.development_seed, args.development_runs)[1] > args.promotion_seed:
        raise ValueError("development and promotion seed panels overlap")
    potential_weights = tuple(
        getattr(args, f"potential_{term}_weight") for term in POTENTIAL_TERMS
    )
    probe_env = sts2_sim.Batch(1, args.training_seed, None, ascension=STAGES[0][0])
    source = None
    if args.checkpoint:
        model, source = load(args.checkpoint, target)
        layout = model.layout
        requested = (args.width, args.layers, args.heads, args.feedforward)
        loaded = (model.width, model.layers, model.heads, model.feedforward)
        if any(value is not None and value != saved for value, saved in zip(requested, loaded)):
            raise ValueError(f"checkpoint architecture {loaded} does not match requested {requested}")
        requested_pooling = {
            name: getattr(args, name + "_pooling") for name in POOLING_DEFAULTS
            if getattr(args, name + "_pooling") is not None
        }
        if any(model.pooling[name] != mode for name, mode in requested_pooling.items()):
            raise ValueError("checkpoint pooling configuration cannot be changed")
        if model.potential_weights != potential_weights:
            raise ValueError("checkpoint potential weights cannot be changed")
        args.width, args.layers, args.heads, args.feedforward = loaded
    else:
        config = tuple(value if value is not None else default for value, default in zip(
            (args.width, args.layers, args.heads, args.feedforward), DEFAULT_ARCHITECTURE
        ))
        args.width, args.layers, args.heads, args.feedforward = config
        layout = dict(probe_env.token_layout(*config))
        pooling = {name: getattr(args, name + "_pooling") or default
                   for name, default in POOLING_DEFAULTS.items()}
        model = Agent(layout, *config, pooling=pooling,
                      potential_weights=potential_weights).to(target)
    late_stage = (source["stage"] if source else args.start_stage) >= 5
    if (late_stage or args.freeze_backbone) and args.samplers == 1 and args.envs % 2 == 0:
        args.samplers, args.sampler_threads = 2, min(args.sampler_threads, 4)
    if late_stage or args.freeze_backbone:
        args.save_decisions = max(args.save_decisions, 262_144)
    if args.freeze_backbone:
        args.publish_updates = max(args.publish_updates, 16)
        model.requires_grad_(False)
        model.policy.requires_grad_(True)
        model.critic.requires_grad_(True)
    fused_optimizer = target.type != "cpu"
    parameter_groups, group_indices = optimizer_groups(
        model, args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier,
    )
    if args.critic_only:
        model.requires_grad_(False); model.critic.requires_grad_(True)
        parameter_groups = [{"params": list(model.critic.parameters()), "lr_scale": 1.}]
    if source:
        require_optimizer_config(source)
    optimizer = torch.optim.Adam(
        parameter_groups, lr=args.learning_rate, fused=fused_optimizer,
        betas=OPTIMIZER_CONFIG["betas"], eps=OPTIMIZER_CONFIG["eps"],
    )
    optimizer_restored = bool(source and (
        optimizer_compatible(source["optimizer"], len(tuple(model.critic.parameters())))
        if args.critic_only else source["_optimizer_compatible"]
        and repartition_optimizer(source["optimizer"], group_indices)
    ))
    if args.critic_only and source and not optimizer_restored:
        optimizer_restored = repartition_optimizer(source["optimizer"], group_indices, 2)
    if optimizer_restored:
        optimizer.load_state_dict(source["optimizer"])
        torch.set_rng_state(source["torch_rng"].cpu())
    scales = (args.critic_learning_rate_multiplier,) if args.critic_only else (
        1., args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier,
    )
    for group, scale in zip(optimizer.param_groups, scales):
        group["lr"] = args.learning_rate * scale
        group["lr_scale"] = scale
        group["betas"] = OPTIMIZER_CONFIG["betas"]
        group["eps"] = OPTIMIZER_CONFIG["eps"]
        group["fused"] = fused_optimizer
        group["foreach"] = None
    output = Path(args.output)
    continuing = output.exists()
    if continuing and (not source or Path(args.checkpoint).resolve() != (output / "latest.pt").resolve()):
        raise FileExistsError(f"{output} exists; continue it from its latest.pt")
    output.mkdir(parents=True, exist_ok=continuing)
    training_lock = (output / ".train.lock").open("a+")
    try:
        fcntl.flock(training_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        raise RuntimeError(f"trainer already running for {output}") from error
    training_lock.seek(0); training_lock.truncate(); training_lock.write(str(os.getpid())); training_lock.flush()
    training = {key: value for key, value in vars(args).items() if key != "command"}
    parent_checkpoint_sha256 = sha256_file(Path(args.checkpoint)) if source else None
    parent = checkpoint_origin(Path(args.checkpoint), parent_checkpoint_sha256) if source else None
    run_id = uuid.uuid4().hex
    manifest = json.loads((output / "run.json").read_text()) if continuing else {
        "schema": 2, "run_id": run_id,
        "lineage_id": source.get("lineage_id") if source and source.get("lineage_id") else run_id,
        "model_version": model.model_version, "feature_version": FEATURE_VERSION,
        "fingerprint": probe_env.fingerprint(), "layout": layout, "precision": args.precision,
        "change": CHANGE, "parameters": sum(parameter.numel() for parameter in model.parameters()),
        "architecture": architecture(model),
        "stages": [{"ascension": ascension, "bonus": bonus} for ascension, bonus in STAGES],
        "training": training,
        "source": args.checkpoint, "optimizer_restored": optimizer_restored,
        "resume": ("warm continuation; learner, optimizer and reservoir restored; sampler restarted"
                   if optimizer_restored else
                   "warm continuation; learner and reservoir restored; optimizer reset")
                  if source else None,
    }
    manifest.setdefault("run_id", run_id)
    manifest.setdefault("lineage_id", manifest["run_id"])
    manifest.setdefault("telemetry_window_decisions", 32_768)
    if parent and not parent.get("run_id") and continuing:
        parent["run_id"] = manifest["run_id"]
    if continuing and manifest.get("model_version") != model.model_version:
        raise ValueError("new model versions require a new output directory")
    manifest["schema"] = 2
    manifest["model_version"] = model.model_version
    manifest["precision"] = args.precision
    manifest["parameters"] = sum(parameter.numel() for parameter in model.parameters())
    manifest["architecture"] = architecture(model)
    evaluation_panels = {
        "development": {"seed": args.development_seed, "runs_per_character": args.development_runs},
        "promotion": {
            "seed": args.promotion_seed, "stride": 100_000_000,
            "runs_per_character": args.promotion_runs,
        },
    }
    if continuing and manifest.get("evaluation_panels") not in (None, evaluation_panels):
        raise ValueError("evaluation seed panels cannot change within a run")
    manifest["evaluation_panels"] = evaluation_panels
    manifest["curriculum"] = {
        "stage_local_decay_decisions": args.progress_decisions,
        "progress_beta": (
            f"fixed {args.progress_beta}; off after A0/+0 promotion"
            if args.progress_beta is not None
            else "run-global 1 -> 0.1; held through A0/+0; off on A1/+0 entry"
        ),
        "entropy": f"fixed {args.entropy_weight}" if args.entropy_weight is not None
                   else "(start -> end) * max(0.35, 0.8^stage)",
    }
    sessions = manifest.setdefault("sessions", [])
    previous_reports = logged_history(output, manifest)[0] if continuing else {}
    elapsed_offset = float((source or {}).get("training_elapsed_seconds", max(
        (row["metrics"].get("seconds", 0) for row in previous_reports.values()), default=0,
    )))
    event_dir = output / "events"; event_dir.mkdir(exist_ok=True)
    saved_ids = [int(row["id"]) for row in sessions if str(row.get("id", "")).isdigit()]
    file_ids = [int(path.stem) for path in event_dir.glob("*.jsonl") if path.stem.isdigit()]
    args.trainer_session = max(saved_ids + file_ids, default=0) + 1
    event_log = event_dir / f"{args.trainer_session:06}.jsonl"
    descriptor = os.open(event_log, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    os.close(descriptor)
    sessions.append({
        "id": args.trainer_session, "step": source["decisions"] if source else 0,
        "log": str(event_log.relative_to(output)), "started_at": time.time(),
        "parent": parent, "training": training, "optimizer_restored": optimizer_restored,
    })
    atomic_json(output / "run.json", manifest)
    args.run_id = manifest["run_id"]
    args.event_log = str(event_log)
    revisions = {
        key: int((source or {}).get("revisions", {}).get(key, 0))
        for key in ("update_attempt", "weights_revision", "policy_revision", "actor_revision")
    }
    configure_logging(
        output, "learner", args.log_level, args.trainer_session, manifest["run_id"], event_log,
        {
        "event": "session_start", "model_version": model.model_version,
        "step": source["decisions"] if source else 0,
        "resolved_decisions_total": source["decisions"] if source else 0,
        "stage": source["stage"] if source else args.start_stage,
        "training_elapsed_seconds": elapsed_offset,
        "checkpoint": args.checkpoint,
        "optimizer_restored": optimizer_restored,
        "parent_checkpoint_step": source["decisions"] if source else None,
        "parent_checkpoint_sha256": parent_checkpoint_sha256, "parent": parent,
        "revisions": revisions, "training": training,
        },
    )
    checkpoints_dir = output / "checkpoints"; checkpoints_dir.mkdir(exist_ok=continuing)
    development_dir = output / "development"; development_dir.mkdir(exist_ok=continuing)
    champions_dir = output / "stage-champions"; champions_dir.mkdir(exist_ok=continuing)
    entries_dir = output / "stage-entries"; entries_dir.mkdir(exist_ok=continuing)
    stage = source["stage"] if source else args.start_stage
    reservoir = WinningReservoir(args.winning_capacity, args.envs)
    if source and source.get("winning_reservoir"):
        reservoir.load_state_dict(source["winning_reservoir"])
        missing = [index for index, row in enumerate(reservoir.rows) if row[4] is None]
        for start in range(0, len(missing), 128):
            indices = missing[start:start + 128]
            with torch.no_grad():
                inputs = unpack([reservoir.rows[index][0] for index in indices], target, model)
                logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[0].masked_fill(~inputs[6], -torch.inf)
                policies = logits.log_softmax(-1).cpu().numpy()
            for index, policy in zip(indices, policies):
                row = reservoir.rows[index]
                reservoir.rows[index] = (*row[:4], np.asarray(
                    policy[:packed_action_count(row[0])], np.float16,
                ))
    decisions = source["decisions"] if source else 0
    auxiliary_decisions = source.get("auxiliary_decisions", source["decisions"]) if source else 0
    stage_decisions, progress_active = resume_curriculum(source, stage)
    critic_balance = CriticBalance(
        args.critic_balance_decay,
        source.get("critic_balance") if source else None,
    )
    sampler_session = source.get("sampler_session", source.get("sampler_index", 0)) if source else 0
    promotion_index = source.get("promotion_index", 0) if source else 0
    if not 0 <= stage < len(STAGES):
        raise ValueError("invalid checkpoint stage")
    if args.decisions and args.decisions < decisions:
        raise ValueError("--decisions precedes the checkpoint")
    if args.promote_now and (not source or args.decisions != decisions):
        raise ValueError("--promote-now requires an unchanged checkpoint decision target")
    latest = output / "latest.pt"
    training_started = [None]
    def write_checkpoint(path, step, kind):
        checkpoint_id = uuid.uuid4().hex
        elapsed = elapsed_offset + (
            time.monotonic() - training_started[0] if training_started[0] else 0
        )
        digest = save_checkpoint(
            path, model, optimizer, manifest, stage, step, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions + step - decisions,
            stage_decisions + step - decisions, critic_balance, False,
            revisions, elapsed, checkpoint_id,
        )
        event_end = emit_event({
            "event": "checkpoint", "checkpoint_id": checkpoint_id, "kind": kind,
            "step": step, "resolved_decisions_total": step, "stage": stage,
            "update_attempt": revisions["update_attempt"],
            "weights_revision": revisions["weights_revision"],
            "policy_revision": revisions["policy_revision"],
            "actor_revision": revisions["actor_revision"],
            "training_elapsed_seconds": elapsed,
            "path": str(path.relative_to(output)), "sha256": digest,
        })
        sync_events()
        return {
            "step": step, "stage": stage, "sampler_session": sampler_session,
            "checkpoint": str(path.relative_to(output)), "sha256": digest,
            "checkpoint_id": checkpoint_id, "run_id": manifest["run_id"],
            "session_id": args.trainer_session,
            "event_log": str(event_log.relative_to(output)), "event_end": event_end,
            "revisions": dict(revisions), "training_elapsed_seconds": elapsed,
        }

    def activate_checkpoint(record):
        atomic_json(output / "latest.json", record | {"alias": latest.name})
        link_checkpoint(output / record["checkpoint"], latest)

    if not continuing:
        initial_checkpoint = checkpoints_dir / (
            f"{decisions:012}-w{revisions['weights_revision']:012}.pt"
        )
        initial_record = write_checkpoint(initial_checkpoint, decisions, "initial")
        atomic_json(output / "initial.json", initial_record)
        link_checkpoint(initial_checkpoint, output / "initial.pt")
        activate_checkpoint(initial_record)
        active_checkpoint = initial_record
    elif (output / "latest.json").exists():
        active_checkpoint = json.loads((output / "latest.json").read_text())
    else:
        migrated = checkpoints_dir / (
            f"{decisions:012}-w{revisions['weights_revision']:012}.pt"
        )
        active_checkpoint = write_checkpoint(migrated, decisions, "migration")
        activate_checkpoint(active_checkpoint)
    started = time.monotonic()
    training_started[0] = started
    run_started = started - elapsed_offset
    deadline = started + args.hours * 3600 if args.hours else math.inf
    training_seconds = promotion_seconds = 0.0
    best_path = output / "best.json"
    best = json.loads(best_path.read_text()) if best_path.exists() else {}
    stage_bests = load_stage_bests(output, development_dir, checkpoints_dir, best)
    for key, record in stage_bests.items():
        champion = champions_dir / f"{int(key):02}.pt"
        if not champion.exists():
            link_checkpoint(output / record["checkpoint"], champion)
        record["artifact"] = str(champion.relative_to(output))
    if stage_bests != best.get("stages", {}):
        best["stages"] = stage_bests
        atomic_json(best_path, best)
    def promote(result=None):
        nonlocal stage, stage_decisions, promotion_index, progress_active, promotion_seconds
        nonlocal active_checkpoint
        if decisions in logged_history(output, manifest)[1] or stage + 1 >= len(STAGES):
            raise ValueError("promotion is not available at this checkpoint")
        promotion_started = time.monotonic()
        manual = result is None
        if manual:
            panels = sorted(development_dir.glob("*.json"))
            result = json.loads(panels[-1].read_text())["result"] if panels else {
                "runs": 0, "wins": 0, "characters": [],
            }
        promotion_index += 1
        promotion = {
            "schema": 1, "step": decisions, "seed": None,
            "seconds": elapsed_offset + time.monotonic() - started,
            "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
            "trigger": {
                "mode": "manual" if manual else "last sampled trajectories",
                "trajectories": args.promotion_window,
                "minimum_per_character": 1,
                "win_rate": args.promote_win_rate,
            },
            "threshold": args.promote_win_rate, "promoted": True, "result": result,
        }
        progress_active &= stage != 6
        stage += 1
        stage_decisions = 0
        if stage == 5:
            if args.samplers == 1 and args.envs % 2 == 0:
                args.samplers, args.sampler_threads = 2, min(args.sampler_threads, 4)
            args.save_decisions = max(args.save_decisions, 262_144)
        reservoir.clear()
        promotion["next_stage"] = {
            "index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1],
        }
        emit_event({"event": "promotion", "resolved_decisions_total": decisions, **promotion})
        entry = entries_dir / (
            f"{stage:02}-{decisions:012}-w{revisions['weights_revision']:012}.pt"
        )
        active_checkpoint = write_checkpoint(entry, decisions, "stage_entry")
        best.setdefault("entries", {})[str(stage)] = {
            "step": decisions,
            "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
            "checkpoint": str(entry.relative_to(output)),
            "sha256": active_checkpoint["sha256"],
        }
        atomic_json(best_path, best)
        promotion_seconds += time.monotonic() - promotion_started
        activate_checkpoint(active_checkpoint)

    if args.promote_now:
        promote()
    while time.monotonic() < deadline and (not args.decisions or decisions < args.decisions):
        sampler_session += 1
        base = decisions
        budget = args.decisions - decisions if args.decisions else (1 << 62) // args.envs * args.envs
        last_checkpoint = None
        def save_step(step):
            nonlocal last_checkpoint, active_checkpoint
            checkpoint = checkpoints_dir / (
                f"{step:012}-w{revisions['weights_revision']:012}.pt"
            )
            active_checkpoint = write_checkpoint(checkpoint, step, "periodic")
            activate_checkpoint(active_checkpoint)
            last_checkpoint = active_checkpoint
        def save_report(point, pipeline, window):
            written = time.time()
            row = {
                "schema": 1, "step": point["steps"], "window": window,
                "sampler_session": sampler_session,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "description": f"Continuous V{model.model_version} training at A{STAGES[stage][0]}/+{STAGES[stage][1]}.",
                "pipeline": pipeline, "metrics": point,
            }
            emit_event({"event": "report", **row}, event_time=written)
            keep = {output / active_checkpoint["checkpoint"]}
            initial = json.loads((output / "initial.json").read_text())
            keep.add(output / initial["checkpoint"])
            keep.update(output / row["checkpoint"] for row in stage_bests.values())
            if best.get("checkpoint"):
                keep.add(output / best["checkpoint"])
            for checkpoint in checkpoints_dir.glob("*.pt"):
                if checkpoint not in keep:
                    checkpoint.unlink()
        model.train()
        training = train_stream(
            model, optimizer, args, sampler_session, stage, target, deadline, budget,
            base, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
            save_report, save_step, manifest["fingerprint"], critic_balance, revisions,
        )
        if not training["decisions"]:
            break
        decisions += training["decisions"]
        auxiliary_decisions += training["decisions"]
        stage_decisions += training["decisions"]
        training_seconds += training["seconds"]
        if not training["promotion_ready"] or time.monotonic() >= deadline:
            break
        promote(training["promotion_result"])
    current = (decisions, stage, revisions["weights_revision"])
    saved = (
        active_checkpoint.get("step"), active_checkpoint.get("stage"),
        active_checkpoint.get("revisions", {}).get("weights_revision"),
    )
    if current != saved:
        final_path = checkpoints_dir / (
            f"{decisions:012}-w{revisions['weights_revision']:012}.pt"
        )
        active_checkpoint = write_checkpoint(final_path, decisions, "final")
    activate_checkpoint(active_checkpoint)
    emit_event({
        "event": "session_complete", "decisions": decisions,
        "resolved_decisions_total": decisions,
        "stage": stage, "training_seconds": training_seconds,
        "promotion_seconds": promotion_seconds,
        "training_fraction": training_seconds / max(1e-9, training_seconds + promotion_seconds),
        "training_elapsed_seconds": elapsed_offset + time.monotonic() - started,
        **revisions,
    })
    shutdown_logging()


def diagnose_search(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    env = sts2_sim.Batch(args.envs, args.seed, None, ascension=ascension)
    env.set_training_bonus(bonus)
    env.load_policy(export_value_model(None, model, env.fingerprint(), 1, 0, True))
    previous = [-1] * args.envs
    roots = []
    for step in range(args.max_steps):
        stats = env.stats(); seeds = env.seeds()
        for index, row in enumerate(stats):
            turn = row[5] if row[4] == 1 else -1
            fresh = turn >= 0 and turn != previous[index]
            previous[index] = turn
            ticket = (seeds[index] * 0x9E3779B1 + row[1] * 131 + turn * 17) & 0xffffffff
            if not fresh or ticket % args.sample_stride or len(roots) >= args.roots:
                continue
            started = time.monotonic()
            try:
                result = json.loads(env.search_diagnostics(
                    index, args.simulations, 1, args.max_depth, args.batch_size,
                    args.prior_temperature, args.exploration, args.policy_temperature,
                    args.exact_samples, args.max_exact_states, args.seed + len(roots),
                    args.progress_value, args.q_temperature,
                ))
            except ValueError as error:
                print(json.dumps({"skipped": {"seed": seeds[index], "floor": row[1], "turn": turn},
                                  "error": str(error)}), flush=True)
                continue
            result["seconds"] = time.monotonic() - started
            roots.append(result)
            print(json.dumps({"root": result["root"], "mcts": result["mcts"],
                              "exact": result["exact"], "seconds": result["seconds"]}), flush=True)
        if len(roots) >= args.roots:
            break
        result = env.policy(args.policy_temperature, True, True)
        done = np.flatnonzero(result[8]).tolist()
        if done:
            env.reset(done, 0)
            for index in done:
                previous[index] = -1
    if not roots:
        raise RuntimeError("no turn-start roots sampled")
    output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
    raw = output.with_suffix(".json")
    raw.write_text(json.dumps({"checkpoint": args.checkpoint, "roots": roots}, indent=2))
    nodes = [node | {"root": root_index, **root["root"]}
             for root_index, root in enumerate(roots) for node in root["nodes"]]
    pairs = []; total_pairs = 0; rng = np.random.default_rng(args.seed)
    for root_index, root in enumerate(roots):
        leaves = root["leaves"]
        for left in range(len(leaves)):
            for right in range(left + 1, len(leaves)):
                total_pairs += 1
                high, low = (leaves[left], leaves[right]) \
                    if leaves[left]["value"] >= leaves[right]["value"] \
                    else (leaves[right], leaves[left])
                pair = {
                    "root": root_index,
                    "delta_value": high["value"] - low["value"],
                    "delta_player_hp": high["player_hp"] - low["player_hp"],
                    "delta_enemy_hp": high["enemy_hp"] - low["enemy_hp"],
                    "weight": high["weight"] * low["weight"],
                }
                if len(pairs) < args.max_pairs:
                    pairs.append(pair)
                else:
                    replace = int(rng.integers(total_pairs))
                    if replace < args.max_pairs:
                        pairs[replace] = pair
    leaf_fits = []
    for root_index, root in enumerate(roots):
        value = np.asarray([leaf["value"] for leaf in root["leaves"]])
        features = np.asarray([[1, leaf["player_hp"], leaf["enemy_hp"]] for leaf in root["leaves"]])
        coefficient = np.linalg.lstsq(features, value, rcond=None)[0]
        residual = np.square(value - features @ coefficient).sum()
        total = np.square(value - value.mean()).sum()
        leaf_fits.append({"root": root_index, "value_per_player_hp": float(coefficient[1]),
                          "value_per_enemy_hp": float(coefficient[2]),
                          "r2": float(1 - residual / total) if total else None})
    exact = np.asarray([node["exact"] for node in nodes]); approximate = np.asarray([node["approximate"] for node in nodes])
    errors = np.abs(exact - approximate); visits = np.asarray([node["visits"] for node in nodes])
    actions = [action | {"root": root_index, "node": node["node"], "depth": node["depth"]}
               for root_index, root in enumerate(roots) for node in root["nodes"]
               for action in node["actions"] if action["approximate"] is not None]
    action_exact = np.asarray([action["exact"] for action in actions])
    action_approximate = np.asarray([action["approximate"] for action in actions])
    action_errors = np.abs(action_exact - action_approximate)
    gaps = [root["root_policy"] for root in roots]
    summary = {
        "objective": "progress" if args.progress_value else "win",
        "roots": len(roots), "nodes": len(nodes), "leaf_pairs": total_pairs,
        "plotted_leaf_pairs": len(pairs), "mae": float(errors.mean()),
        "rmse": float(np.sqrt(np.mean((exact - approximate) ** 2))),
        "correlation": float(np.corrcoef(exact, approximate)[0, 1]) if len(nodes) > 1 else None,
        "visit_error_correlation": float(np.corrcoef(np.log1p(visits), errors)[0, 1]) if len(nodes) > 1 else None,
        "policy_expected_gap_mean": float(np.mean([row["policy_expected_gap"] for row in gaps])),
        "policy_greedy_gap_mean": float(np.mean([row["policy_greedy_gap"] for row in gaps])),
        "search_gap_mean": float(np.mean([row["search_gap"] for row in gaps])),
        "search_q_gap_mean": float(np.mean([row["search_q_gap"] for row in gaps])),
        "q_actions": len(actions), "q_mae": float(action_errors.mean()),
        "q_rmse": float(np.sqrt(np.mean((action_exact - action_approximate) ** 2))),
        "q_correlation": float(np.corrcoef(action_exact, action_approximate)[0, 1]),
        "q_target_gap_mean": float(np.mean([row["q_target_gap"] for row in gaps])),
        "q_target_entropy_mean": float(np.mean([row["q_target_entropy"] for row in gaps])),
        "leaf_value_linear_fits": leaf_fits,
    }
    if args.progress_value:
        summary |= {"mae_floors": summary["mae"] * 52,
                    "rmse_floors": summary["rmse"] * 52,
                    "policy_expected_gap_floors": summary["policy_expected_gap_mean"] * 52,
                    "search_gap_floors": summary["search_gap_mean"] * 52,
                    "search_q_gap_floors": summary["search_q_gap_mean"] * 52,
                    "q_mae_floors": summary["q_mae"] * 52,
                    "q_rmse_floors": summary["q_rmse"] * 52,
                    "q_target_gap_floors": summary["q_target_gap_mean"] * 52}
    data = json.dumps({"nodes": nodes, "pairs": pairs, "roots": roots, "summary": summary}).replace("</", "<\\/")
    html = output.with_suffix(".html")
    html.write_text("""<!doctype html><meta charset=utf-8><title>Search diagnostics</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(520px,1fr));gap:18px}.plot{height:560px;background:#171d28;border:1px solid #303a49;border-radius:10px}pre{white-space:pre-wrap}</style>
<h1>Exact vs approximate expectimax</h1><p>Crosses have zero outgoing visits: MCTS expanded and evaluated the node, backed its value up through the incoming edge, but never traversed an action from that node. Their approximate value is therefore the raw value-model estimate.</p><pre id=summary></pre><div id=values class=grid></div><h1>Exact versus sampled action Q</h1><div id=actions class=grid></div><h1>Value geometry across exhaustive leaves</h1><p>Signed HP differences are better-valued leaf minus worse-valued leaf. Negative enemy HP therefore means the value model prefers a leaf with less enemy life remaining.</p><div id=leaves class=grid></div><script>const data=""" + data + r""",config={responsive:true,displaylogo:false},base={template:'plotly_dark',paper_bgcolor:'#171d28',plot_bgcolor:'#171d28',margin:{l:70,r:30,t:55,b:60}};
summary.textContent=JSON.stringify(data.summary,null,2);const progress=data.summary.objective==='progress',scale=progress?53:1,valueName=progress?'predicted terminal category':'win probability';function panel(parent,id){const div=document.createElement('div');div.id=id;div.className='plot';document.querySelector(parent).append(div);return div}data.roots.forEach((root,index)=>{const label=`Root ${index} · character ${root.root.character} · floor ${root.root.floor} turn ${root.root.turn}`,n=data.nodes.filter(row=>row.root===index),x=n.map(row=>row.exact*scale),y=n.map(row=>row.approximate*scale),lo=Math.min(...x,...y),hi=Math.max(...x,...y),valuePanel=panel('#values',`values-${index}`);Plotly.newPlot(valuePanel,[{x,y,mode:'markers',marker:{size:n.map(row=>row.visits?5+3*Math.log1p(row.visits):8),symbol:n.map(row=>row.visits?'circle':'x'),color:n.map(row=>Math.log1p(row.visits)),colorscale:'Viridis',showscale:true,colorbar:{title:'log(1+visits)'}},customdata:n.map(row=>[row.depth,row.visits,row.absolute_error*scale,row.rng]),hovertemplate:'depth %{customdata[0]} · visits %{customdata[1]}<br>exact %{x:.6f}<br>approximate %{y:.6f}<br>|error| %{customdata[2]:.6f}<br>RNG %{customdata[3]}<extra></extra>'},{x:[lo,hi],y:[lo,hi],mode:'lines',line:{dash:'dash',color:'#ddd'},name:'identity'}],{...base,title:label,xaxis:{title:`Repeated-exhaustive ${valueName}`,gridcolor:'#303a49'},yaxis:{title:`MCTS ${valueName}`,gridcolor:'#303a49'},showlegend:false},config);const q=root.nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null),qx=q.map(row=>row.exact*scale),qy=q.map(row=>row.approximate*scale),qlo=Math.min(...qx,...qy),qhi=Math.max(...qx,...qy),actionPanel=panel('#actions',`actions-${index}`);Plotly.newPlot(actionPanel,[{x:qx,y:qy,mode:'markers',marker:{size:q.map(row=>5+2.5*Math.log1p(row.visits)),color:q.map(row=>Math.log1p(row.visits)),colorscale:'Viridis',showscale:true,colorbar:{title:'log(1+visits)'},opacity:.65},customdata:q.map(row=>[row.action,row.depth,row.visits,row.target]),hovertemplate:'%{customdata[0]}<br>depth %{customdata[1]} · visits %{customdata[2]}<br>exact Q %{x:.6f}<br>sampled Q %{y:.6f}<br>target %{customdata[3]:.4f}<extra></extra>'},{x:[qlo,qhi],y:[qlo,qhi],mode:'lines',line:{dash:'dash',color:'#ddd'},name:'identity'}],{...base,title:label,xaxis:{title:`Repeated-exhaustive Q (${valueName})`,gridcolor:'#303a49'},yaxis:{title:`Sampled expectimax Q (${valueName})`,gridcolor:'#303a49'},showlegend:false},config);const p=data.pairs.filter(row=>row.root===index),z=p.map(row=>row.delta_value*scale),leafPanel=panel('#leaves',`leaves-${index}`);Plotly.newPlot(leafPanel,[{type:'scatter3d',mode:'markers',x:p.map(row=>row.delta_player_hp),y:p.map(row=>row.delta_enemy_hp),z,marker:{size:3,opacity:.35,color:z,colorscale:'Plasma',showscale:true,colorbar:{title:`Δ ${valueName}`}},customdata:p.map(row=>row.weight),hovertemplate:`Δ player HP %{x}<br>Δ enemy HP %{y}<br>Δ ${valueName} %{z:.6f}<br>particle-pair weight %{customdata}<extra></extra>`}],{...base,title:label,scene:{xaxis:{title:'Δ player HP'},yaxis:{title:'Δ enemy remaining HP'},zaxis:{title:`Δ ${valueName}`}}},config)})</script>""")
    print(json.dumps(summary, indent=2)); print(html); print(raw)


def compare_search(args):
    runs = []
    for item in args.input:
        label, path = item.split("=", 1)
        data = json.loads(Path(path).read_text())
        nodes = [node for root in data["roots"] for node in root["nodes"]]
        exact = np.asarray([node["exact"] for node in nodes])
        approximate = np.asarray([node["approximate"] for node in nodes])
        actions = [action for root in data["roots"] for node in root["nodes"]
                   for action in node["actions"] if action["approximate"] is not None]
        action_exact = np.asarray([action["exact"] for action in actions])
        action_approximate = np.asarray([action["approximate"] for action in actions])
        summary = {
            "nodes": len(nodes),
            "mae": float(np.abs(exact - approximate).mean()),
            "correlation": float(np.corrcoef(exact, approximate)[0, 1]),
            "q_actions": len(actions),
            "q_mae": float(np.abs(action_exact - action_approximate).mean()),
            "q_correlation": float(np.corrcoef(action_exact, action_approximate)[0, 1]),
            "search_gap_mean": float(np.mean([root["root_policy"]["search_gap"] for root in data["roots"]])),
            "search_q_gap_mean": float(np.mean([root["root_policy"]["search_q_gap"] for root in data["roots"]])),
            "q_target_gap_mean": float(np.mean([root["root_policy"]["q_target_gap"] for root in data["roots"]])),
        }
        runs.append({"temperature": label, "objective": data["roots"][0]["settings"]["objective"], "summary": summary,
                     "roots": [{"root": root["root"], "nodes": root["nodes"]}
                               for root in data["roots"]]})
    if not runs or any(len(run["roots"]) != len(runs[0]["roots"]) or
                       run["objective"] != runs[0]["objective"] for run in runs):
        raise ValueError("search diagnostics have incompatible roots")
    data = json.dumps(runs).replace("</", "<\\/")
    output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("""<!doctype html><meta charset=utf-8><title>MCTS prior-temperature comparison</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(560px,1fr));gap:18px}.plot{height:560px;background:#171d28;border:1px solid #303a49;border-radius:10px}table{border-collapse:collapse;margin-bottom:24px}th,td{padding:7px 12px;border-bottom:1px solid #303a49;text-align:right}th:first-child,td:first-child{text-align:left}</style>
<h1>MCTS prior-temperature comparison</h1><table id=summary></table><h1>Node values</h1><div id=plots class=grid></div><h1>Action Q values</h1><div id=qplots class=grid></div><script>const runs=""" + data + r""",scale=runs[0].objective==='progress'?53:1,unit=runs[0].objective==='progress'?'terminal category':'win probability',colors=['#60a5fa','#f59e0b','#f472b6','#34d399'],config={responsive:true,displaylogo:false},base={template:'plotly_dark',paper_bgcolor:'#171d28',plot_bgcolor:'#171d28',margin:{l:70,r:25,t:55,b:60}};summary.innerHTML=`<tr><th>Prior temperature</th><th>Nodes</th><th>Node MAE (${unit})</th><th>Node corr.</th><th>Q MAE (${unit})</th><th>Q corr.</th><th>Visit gap</th><th>Q argmax gap</th><th>Q target gap</th></tr>`+runs.map(run=>`<tr><td>${run.temperature}</td><td>${run.summary.nodes}</td><td>${Number(run.summary.mae*scale).toPrecision(4)}</td><td>${Number(run.summary.correlation).toFixed(4)}</td><td>${Number(run.summary.q_mae*scale).toPrecision(4)}</td><td>${Number(run.summary.q_correlation).toFixed(4)}</td><td>${Number(run.summary.search_gap_mean*scale).toPrecision(4)}</td><td>${Number(run.summary.search_q_gap_mean*scale).toPrecision(4)}</td><td>${Number(run.summary.q_target_gap_mean*scale).toPrecision(4)}</td></tr>`).join('');for(let root=0;root<runs[0].roots.length;root++){const div=document.createElement('div');div.className='plot';plots.append(div);const all=runs.flatMap(run=>run.roots[root].nodes),lo=Math.min(...all.flatMap(row=>[row.exact,row.approximate]))*scale,hi=Math.max(...all.flatMap(row=>[row.exact,row.approximate]))*scale,info=runs[0].roots[root].root,traces=runs.map((run,index)=>{const rows=run.roots[root].nodes;return{x:rows.map(row=>row.exact*scale),y:rows.map(row=>row.approximate*scale),mode:'markers',name:`temperature ${run.temperature}`,marker:{color:colors[index%colors.length],size:rows.map(row=>row.visits?5+2.5*Math.log1p(row.visits):7),symbol:rows.map(row=>row.visits?'circle':'x'),opacity:.6},customdata:rows.map(row=>[row.depth,row.visits,row.absolute_error*scale]),hovertemplate:`temperature ${run.temperature}<br>depth %{customdata[0]} · visits %{customdata[1]}<br>exact %{x:.6f}<br>approximate %{y:.6f}<br>|error| %{customdata[2]:.6f}<extra></extra>`}});traces.push({x:[lo,hi],y:[lo,hi],mode:'lines',name:'identity',line:{color:'#ddd',dash:'dash'}});Plotly.newPlot(div,traces,{...base,title:`Root ${root} · character ${info.character} · floor ${info.floor} turn ${info.turn}`,xaxis:{title:`Repeated-exhaustive ${unit}`,gridcolor:'#303a49',range:[lo,hi]},yaxis:{title:`MCTS ${unit}`,gridcolor:'#303a49',range:[lo,hi]},legend:{orientation:'h'}},config);const qdiv=document.createElement('div');qdiv.className='plot';qplots.append(qdiv);const qall=runs.flatMap(run=>run.roots[root].nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null)),qlo=Math.min(...qall.flatMap(row=>[row.exact,row.approximate]))*scale,qhi=Math.max(...qall.flatMap(row=>[row.exact,row.approximate]))*scale,qtraces=runs.map((run,index)=>{const rows=run.roots[root].nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null);return{x:rows.map(row=>row.exact*scale),y:rows.map(row=>row.approximate*scale),mode:'markers',name:`temperature ${run.temperature}`,marker:{color:colors[index%colors.length],size:rows.map(row=>5+2.5*Math.log1p(row.visits)),opacity:.55},customdata:rows.map(row=>[row.action,row.depth,row.visits,row.target]),hovertemplate:`temperature ${run.temperature}<br>%{customdata[0]}<br>depth %{customdata[1]} · visits %{customdata[2]}<br>exact Q %{x:.6f}<br>sampled Q %{y:.6f}<br>target %{customdata[3]:.4f}<extra></extra>`}});qtraces.push({x:[qlo,qhi],y:[qlo,qhi],mode:'lines',name:'identity',line:{color:'#ddd',dash:'dash'}});Plotly.newPlot(qdiv,qtraces,{...base,title:`Root ${root} · character ${info.character} · floor ${info.floor} turn ${info.turn}`,xaxis:{title:`Repeated-exhaustive Q (${unit})`,gridcolor:'#303a49',range:[qlo,qhi]},yaxis:{title:`Sampled expectimax Q (${unit})`,gridcolor:'#303a49',range:[qlo,qhi]},legend:{orientation:'h'}},config)}</script>""")
    print(output)


def value_dataset(model, args, target, seed, runs, deadline):
    states = []; targets = []; terminal_values = []; potentials = []; characters = []
    reports = []
    model.eval()
    for character in range(5):
        outcomes = []; floors = []; caps = 0
        for start in range(0, runs, args.batch):
            if time.monotonic() >= deadline:
                raise TimeoutError("finalization exceeded --hours")
            count = min(args.batch, runs - start)
            env = sts2_sim.Batch(count, seed + character * 1_000_000 + start, character, ascension=10)
            env.set_potential_weights(model.potential_weights)
            active = np.ones(count, bool)
            episode_steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            pending = [[] for _ in range(count)]
            seen = np.zeros(count, np.int32)
            rng = np.random.default_rng(seed + character * 1_000_000 + start)
            for _ in range(args.max_steps):
                if time.monotonic() >= deadline:
                    raise TimeoutError("finalization exceeded --hours")
                if not active.any():
                    break
                observation = env.observe_tokens(active.tolist())
                legal = observation_legal(observation)
                empty = active & ~legal.any(1)
                for index in np.flatnonzero(empty):
                    pending[index].clear()
                caps += int(empty.sum()); active &= ~empty
                if not active.any():
                    break
                indices = np.flatnonzero(active)
                packed = pack_batch(observation)
                if len(packed) != count:
                    raise ValueError("packed observation row count mismatch")
                inputs = unpack([packed[index] for index in indices], target, model)
                with torch.no_grad(), torch.autocast(
                    target.type, dtype=torch.bfloat16 if args.precision == "bf16" else torch.float16,
                    enabled=args.precision != "fp32",
                ):
                    logits, _, _ = model(*inputs[:6])
                masked = logits.float().masked_fill(~inputs[6], -torch.inf)
                selected = masked.argmax(1).cpu().numpy()
                choice = np.zeros(count, np.int64); choice[indices] = selected
                for index in np.flatnonzero(active):
                    seen[index] += 1
                    sample = packed[index], float(observation[4][index])
                    if len(pending[index]) < args.states_per_run:
                        pending[index].append(sample)
                    else:
                        slot = int(rng.integers(seen[index]))
                        if slot < args.states_per_run:
                            pending[index][slot] = sample
                in_combat = np.asarray([row[4] == 1 for row in env.stats()])
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                _, done, _ = env.step(choice.tolist(), active.tolist())
                done = np.asarray(done, bool); episode_steps += active
                stats = env.stats()
                still_combat = np.asarray([row[4] == 1 for row in stats])
                capped = active & ~done & (
                    (episode_steps >= args.max_steps)
                    | ((combat_steps >= args.max_combat_steps) & still_combat)
                )
                caps += int(capped.sum())
                for index in np.flatnonzero(done):
                    won = int(stats[index][4] == 12)
                    terminal = CATEGORIES - 1 if won else int(stats[index][9])
                    value = terminal / (CATEGORIES - 1)
                    rows, row_potentials = zip(*pending[index]) if pending[index] else ((), ())
                    states.extend(rows); potentials.extend(row_potentials)
                    targets.extend(value - potential for potential in row_potentials)
                    terminal_values.extend([value] * len(rows))
                    characters.extend([character] * len(pending[index]))
                    outcomes.append(won); floors.append((stats[index][0] - 1) * 17 + stats[index][1])
                for index in np.flatnonzero(capped):
                    pending[index].clear()
                active &= ~(done | capped)
        reports.append({
            "character": character, "runs": runs, "resolved": len(outcomes),
            "wins": sum(outcomes), "win_rate": sum(outcomes) / max(1, runs),
            "caps": caps, "floor_mean": float(np.mean(floors)) if floors else 0,
        })
    if not states:
        raise RuntimeError("no resolved trajectories")
    return (states, np.asarray(targets, np.float32), np.asarray(terminal_values, np.float32),
            np.asarray(potentials, np.float32), np.asarray(characters), reports)


def value_predictions(model, states, potentials, batch, target):
    values = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(states), batch):
            inputs = unpack(states[start : start + batch], target, model)
            values.extend(critic_value(model(*inputs[:6])[1]).cpu().tolist())
    return np.asarray(values, np.float32) + potentials


def calibration(logits, labels):
    logits = torch.as_tensor(logits)
    labels = torch.as_tensor(labels)
    log_temperature = torch.zeros((), requires_grad=True)
    bias = torch.zeros((), requires_grad=True)
    optimizer = torch.optim.Adam([log_temperature, bias], lr=.05)
    for _ in range(500):
        optimizer.zero_grad()
        loss = nn.functional.binary_cross_entropy_with_logits(
            logits / log_temperature.exp() + bias, labels
        )
        loss.backward()
        optimizer.step()
        with torch.no_grad():
            log_temperature.clamp_(-5, 5); bias.clamp_(-15, 15)
    return float(log_temperature.detach().exp()), float(bias.detach())


def value_metrics(logits, labels, characters, temperature, bias, base_rates):
    probabilities = 1 / (1 + np.exp(-np.clip(logits / temperature + bias, -80, 80)))
    baseline = np.asarray([base_rates[character] for character in characters])

    def scores(selected):
        if not selected.any():
            return {"states": 0, "base_rate": None, "brier": None, "ece10": None,
                    "constant_brier": None, "beats_constant": False}
        expected = labels[selected]; predicted = probabilities[selected]
        ece = 0.0
        for index in range(10):
            member = (predicted >= index / 10) & (predicted < (index + 1) / 10)
            if index == 9:
                member |= predicted == 1
            if member.any():
                ece += member.mean() * abs(predicted[member].mean() - expected[member].mean())
        brier = float(np.mean((predicted - expected) ** 2))
        constant = float(np.mean((baseline[selected] - expected) ** 2))
        return {
            "states": int(selected.sum()), "base_rate": float(expected.mean()),
            "brier": brier, "ece10": float(ece), "constant_brier": constant,
            "beats_constant": brier < constant,
        }

    result = scores(np.ones(len(labels), bool))
    result["characters"] = [scores(characters == character) for character in range(5)]
    return result


def finalize(args):
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    if min(args.fit_runs, args.calibration_runs, args.test_runs, args.states_per_run,
           args.batch, args.head_batch, args.epochs, args.max_steps,
           args.max_combat_steps, args.hours) <= 0:
        raise ValueError("invalid finalization settings")
    if args.test_runs < 1000:
        raise ValueError("final test requires at least 1000 runs per character")
    namespaces = sorted((seed, seed + 4_000_000 + runs) for seed, runs in (
        (args.fit_seed, args.fit_runs), (args.calibration_seed, args.calibration_runs),
        (args.test_seed, args.test_runs),
    ))
    if any(left[1] >= right[0] for left, right in zip(namespaces, namespaces[1:])):
        raise ValueError("fit, calibration and test seeds must be disjoint")
    target = device()
    deadline = time.monotonic() + args.hours * 3600
    model, checkpoint = load(args.checkpoint, target)
    if checkpoint["stage"] != len(STAGES) - 1:
        raise ValueError("finalization requires an A10/+0 checkpoint")
    args.precision = args.precision or checkpoint.get("precision", "fp32")
    if args.precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    if args.precision != "fp32" and target.type not in ("mps", "cuda"):
        raise ValueError(f"{args.precision} requires MPS or CUDA")
    fit = value_dataset(model, args, target, args.fit_seed, args.fit_runs, deadline)
    fit_states, fit_report = len(fit[0]), fit[5]
    for parameter in model.parameters():
        parameter.requires_grad_(False)
    for parameter in model.critic.parameters():
        parameter.requires_grad_(True)
    optimizer = torch.optim.Adam(model.critic.parameters(), lr=args.learning_rate)
    rng = np.random.default_rng(args.seed)
    model.train()
    for _ in range(args.epochs):
        if time.monotonic() >= deadline:
            raise TimeoutError("finalization exceeded --hours")
        order = rng.permutation(len(fit[0]))
        for start in range(0, len(order), args.head_batch):
            index = order[start : start + args.head_batch]
            inputs = unpack([fit[0][row] for row in index], target, model)
            labels = torch.as_tensor(fit[1][index], device=target)
            loss = nn.functional.mse_loss(critic_value(model(*inputs[:6])[1]), labels)
            optimizer.zero_grad(set_to_none=True); loss.backward(); optimizer.step()
    del fit
    calibrate = value_dataset(model, args, target, args.calibration_seed, args.calibration_runs, deadline)
    calibration_values = value_predictions(
        model, calibrate[0], calibrate[3], args.head_batch, target
    )
    calibration_labels = (calibrate[2] == 1).astype(np.float32)
    temperature, bias = calibration(calibration_values, calibration_labels)
    base_rates = [
        float(calibration_labels[calibrate[4] == character].mean())
        if (calibrate[4] == character).any() else float(calibration_labels.mean())
        for character in range(5)
    ]
    calibration_metrics = value_metrics(
        calibration_values, calibration_labels, calibrate[4], temperature, bias, base_rates
    )
    calibration_report = calibrate[5]
    del calibrate
    test = value_dataset(model, args, target, args.test_seed, args.test_runs, deadline)
    test_values = value_predictions(model, test[0], test[3], args.head_batch, target)
    test_metrics = value_metrics(
        test_values, test[2] == 1, test[4], temperature, bias, base_rates
    )
    test_metrics["held_out_win_each_character"] = all(row["wins"] for row in test[5])
    if not test_metrics["held_out_win_each_character"]:
        raise RuntimeError("final test requires a held-out win for every character")
    if not test_metrics["beats_constant"] or not all(
        row["beats_constant"] for row in test_metrics["characters"]
    ):
        raise RuntimeError("final test Brier score must beat the per-character constant baseline")
    output = Path(args.output)
    digest = export_value_model(output, model, checkpoint["fingerprint"], temperature, bias)
    parity_env = sts2_sim.Batch(8, args.test_seed + 99_000_000, None, ascension=10)
    parity_env.set_potential_weights(model.potential_weights)
    parity_observation = parity_env.observe_tokens()
    parity_inputs = tensors(parity_observation, target, model)
    with torch.no_grad():
        base = critic_value(predict(model, parity_inputs, "fp32")[1]).cpu().numpy() \
            + np.asarray(parity_observation[4])
        python_values = torch.from_numpy(base / temperature + bias).sigmoid().numpy()
    rust_values = np.asarray(parity_env.rust_values(str(output)))
    parity_error = float(np.max(np.abs(python_values - rust_values)))
    if parity_error > 1e-5:
        raise RuntimeError(f"Rust inference differs by {parity_error}")
    final_checkpoint = checkpoint | {
        "model": model.state_dict(), "optimizer": torch.optim.Adam(model.parameters()).state_dict(),
        "finalized": True,
        "calibration": {"temperature": temperature, "bias": bias},
    }
    final_checkpoint.pop("_optimizer_compatible", None)
    torch.save(final_checkpoint, output.with_suffix(".pt"))
    report = {
        "schema": 1, "model_version": model.model_version, "feature_version": FEATURE_VERSION,
        "source": args.checkpoint, "output": str(output), "sha256": digest,
        "policy": "frozen greedy A10/+0", "temperature": temperature, "bias": bias,
        "rust_pytorch_max_abs": parity_error,
        "fit": {"trajectories": fit_report, "states": fit_states},
        "calibration": calibration_metrics | {"trajectories": calibration_report, "base_rates": base_rates},
        "test": test_metrics | {"trajectories": test[5]},
        "supported_states": "states visited by this frozen player-visible policy",
    }
    atomic_json(output.with_suffix(".json"), report)
    print(json.dumps(report, indent=2))


def probe():
    target = torch.device("cpu")
    assert resume_curriculum(None, 7) == (0, False)
    assert resume_curriculum({
        "decisions": 20, "auxiliary_decisions": 12, "stage_decisions": 4,
        "progress_active": False,
    }, 6) == (4, False)
    assert bounded_mcts_timeout(0, 120) == 60
    assert bounded_mcts_timeout(20, 120) == 20
    assert bounded_mcts_timeout(200, 120) == 60
    assert not sampler_wedged(0, None, 200, 120)
    assert not sampler_wedged(0, 195, 200, 120)
    assert sampler_wedged(0, 180, 200, 120)
    env = sts2_sim.Batch(2, 84, 0)
    layout = dict(env.token_layout())
    assert (layout["version"], layout["model_width"], layout["model_layers"],
            layout["model_heads"], layout["model_feedforward"], layout["state_width"],
            layout["action_width"], layout["entity_summaries"]) == (56, 128, 4, 8, 384, 128, 128, 0)
    compact_layout = dict(env.token_layout(64, 2, 4, 128))
    assert tuple(compact_layout[key] for key in (
        "model_width", "model_layers", "model_heads", "model_feedforward",
        "state_width", "action_width", "head_width",
    )) == (64, 2, 4, 128, 64, 64, 64)
    for name, size in {
        "enemy_position": 33, "power_position": 65, "orb_position": 17,
        "map_floor_position": 65, "deck_origin": 257, "draw_top_position": 257,
        "draw_bottom_position": 257, "relic_position": 257, "wax_position": 257,
        "continuation_position": 257, "crystal_row": 11, "crystal_column": 11,
        "crystal_width": 4, "crystal_height": 4,
    }.items():
        assert layout[name + "_semantic_count"] == size

    for _ in range(3):
        observation = env.observe_tokens()
        legal = np.asarray(observation[3][4])
        choices = [int(np.flatnonzero(row)[0]) for row in legal]
        env.step(choices)
    combat = env.observe_tokens()
    noncombat = sts2_sim.Batch(2, 71, 0).observe_tokens()

    def expected_lengths(observation, pooling):
        domains, action_rows, _positions, _legal = flat_domains(observation)
        batch = len(observation[0])
        counts = np.full(batch, 8, np.int64)
        def rows(domain, predicate=lambda _u: True):
            u, _s, _c, _f, row, scope = domains[DOMAIN[domain]]
            selected = (scope == -1) & predicate(u)
            return u[selected], row[selected]
        def collection(name, domain, predicate, active):
            _u, row = rows(domain, predicate)
            item = np.bincount(row, minlength=batch)
            counts[active] += item[active] if pooling[name] == "global_tokens" else 1
        actor_u, actor_rows = rows("actor")
        combat_rows = np.unique(actor_rows)
        all_rows = np.arange(batch)
        collection("deck", "card", lambda u: u[:, 0] == 0, all_rows)
        collection("relic", "relic", lambda u: u[:, 0] == 0, all_rows)
        collection("potion", "potion", lambda u: u[:, 0] == 0, all_rows)
        for name, zone in (("hand", 1), ("draw", 2), ("discard", 3), ("exhaust", 4)):
            collection(name, "card", lambda u, zone=zone: u[:, 0] == zone, combat_rows)
        collection("orb", "orb", lambda _u: True, combat_rows)
        continuation_u, continuation_rows = rows("continuation")
        for row in range(batch):
            q = len(np.unique(continuation_u[continuation_rows == row, 2]))
            counts[row] += q if pooling["continuation"] == "global_tokens" else 1
        _crystal_u, crystal_rows = rows("crystal")
        counts += np.bincount(crystal_rows, minlength=batch)
        counts += np.bincount(action_rows, minlength=batch)
        for row in combat_rows:
            actors = actor_u[actor_rows == row]
            counts[row] += len(actors)
            for name, enemy in (("friendly_effect", False), ("enemy_effect", True)):
                owners = actors[(actors[:, 1] == 2) == enemy, 0]
                effect_count = 0
                for domain in ("power", "status"):
                    effect_u, effect_rows = rows(domain)
                    effect_count += np.isin(effect_u[effect_rows == row, 0], owners).sum()
                mode = pooling[name]
                counts[row] += 0 if mode.endswith("into_actor") else \
                    effect_count if mode == "global_tokens" else len(owners)
        return counts

    configs = [{}]
    for name in ("relic", "deck", "draw", "exhaust", "discard", "hand", "orb", "potion"):
        configs.extend(({name: mode} for mode in COLLECTION_POOLING))
    for name in ("enemy_effect", "friendly_effect"):
        configs.extend(({name: mode} for mode in EFFECT_POOLING))
    configs.extend(({"continuation": mode} for mode in CONTINUATION_POOLING))
    configs.extend(({"phase": mode} for mode in ("sum", "transformer")))
    configs.extend(({"generation_pool": mode} for mode in ("sum", "transformer")))
    with tempfile.TemporaryDirectory() as parity_directory:
        for number, config in enumerate(configs):
            torch.manual_seed(number)
            model = Agent(layout, pooling=config).eval()
            model.critic.weight.data.normal_(std=.1); model.critic.bias.data.normal_(std=.1)
            for observation in (noncombat, combat):
                inputs = tensors(observation, target, model)
                with torch.no_grad():
                    policy, critic = predict(model, inputs, "fp32")
                assert policy.shape[0] == critic.shape[0] == 2 and critic.shape[1] == 1
                assert torch.isfinite(policy).all() and torch.isfinite(critic).all()
                assert np.array_equal(model._sequence_lengths.cpu(),
                                      expected_lengths(observation, model.pooling))
            exported = Path(parity_directory) / f"{number}.bin"
            export_value_model(exported, model, env.fingerprint(), 1, 0)
            rust = np.asarray(env.rust_values(str(exported)))
            python = (critic_value(critic) + torch.as_tensor(combat[4])).sigmoid().numpy()
            assert np.max(np.abs(python - rust)) <= 1e-5

    model = Agent(layout).eval()
    empty_domains = []
    for values in noncombat[2]:
        values = [np.array(value, copy=True) for value in values]
        values[5][values[4] >= 0] = 0
        empty_domains.append(tuple(values))
    empty_actions = [np.array(value, copy=True) for value in noncombat[3]]
    empty_actions[4].fill(0); empty_actions[5].fill(0)
    no_actions = (*noncombat[:2], tuple(empty_domains), tuple(empty_actions), *noncombat[4:])
    with torch.no_grad():
        policy, critic = predict(model, tensors(no_actions, target, model), "fp32")
    assert policy.shape == empty_actions[5].shape and not policy.any()
    assert critic.shape == (len(noncombat[0]), 1)
    assert np.array_equal(model._sequence_lengths.cpu(), expected_lengths(no_actions, model.pooling))

    sequence = torch.randn(1, 3, model.width)
    forward, _ = model.move_gru(sequence)
    backward, _ = model.move_gru(sequence.flip(1))
    assert not torch.allclose(forward[:, -1], backward[:, -1])
    inputs = tensors(combat, target, model)
    with torch.no_grad():
        original = predict(model, inputs, "fp32")
    permutation = np.arange(np.asarray(combat[3][4]).shape[1])[::-1]
    inverse = np.argsort(permutation)
    permuted_domains = []
    for values in combat[2]:
        values = [np.array(value, copy=True) for value in values]
        scoped = values[4] >= 0
        values[4][scoped] = inverse[values[4][scoped]]
        permuted_domains.append(tuple(values))
    permuted_actions = tuple(np.asarray(value)[:, permutation] for value in combat[3])
    permuted_observation = (combat[0], combat[1], tuple(permuted_domains),
                            permuted_actions, combat[4], combat[5])
    with torch.no_grad():
        permuted = predict(model, tensors(permuted_observation, target, model), "fp32")
    assert torch.allclose(original[0], permuted[0][:, inverse], atol=1e-6)
    assert torch.allclose(original[1], permuted[1], atol=1e-6)

    mixed = {
        "relic": "transformer", "deck": "sum", "draw": "transformer",
        "hand": "sum", "discard": "sum", "exhaust": "transformer",
        "orb": "sum", "potion": "transformer", "friendly_effect": "transformer_into_actor",
        "enemy_effect": "sum_token", "continuation": "global_tokens",
        "phase": "sum", "generation_pool": "sum",
    }
    torch.manual_seed(12)
    model = Agent(layout, pooling=mixed, potential_weights=(.001,) + (0.,) * 12).eval()
    model.critic.weight.data.normal_(std=.1); model.critic.bias.data.normal_(std=.1)
    env.set_potential_weights(model.potential_weights)
    combat = env.observe_tokens()
    inputs = tensors(combat, target, model)
    with torch.no_grad():
        base = critic_value(predict(model, inputs, "fp32")[1]).numpy() + combat[4]
        python = torch.from_numpy(base / .83 - .17).sigmoid().numpy()
    with tempfile.TemporaryDirectory() as directory:
        exported = Path(directory) / "value.bin"
        export_value_model(exported, model, env.fingerprint(), .83, -.17)
        rust = np.asarray(env.rust_values(str(exported)))
        invalid_binary = bytearray(exported.read_bytes())
        cap_offset = 8 + 16 + 40 + 64 * 4 + len(SEMANTIC_NAMES) * 4
        struct.pack_into("<I", invalid_binary, cap_offset,
                         struct.unpack_from("<I", invalid_binary, cap_offset)[0] + 1)
        invalid_path = Path(directory) / "invalid.bin"
        invalid_path.write_bytes(invalid_binary)
        try:
            env.rust_values(str(invalid_path))
            raise AssertionError("accepted incompatible binary architecture")
        except (OSError, ValueError):
            pass
        env.load_policy(export_value_model(None, model, env.fingerprint(), 1, 0, actor=True))
        assert len(env.policy(sample=False, advance=False)[0]) == 2
        cached = env.policy(sample=False, advance=False, cache_features=True)
        cached_inputs = cached_tensors(cached[-1], model.width)
        with torch.no_grad():
            expected = predict(model, tensors(combat, target, model), "fp32", flat_policy=True)
            actual = predict_cached(model, cached_inputs, "fp32")
        assert all(torch.allclose(left, right, atol=1e-5) for left, right in zip(actual, expected))
        groups, group_indices = optimizer_groups(model, 1, 1)
        optimizer = torch.optim.Adam(groups)
        sum(parameter.sum() for parameter in model.critic.parameters()).backward()
        optimizer.step()
        optimizer_state = optimizer.state_dict()
        checkpoint = Path(directory) / "checkpoint.pt"
        torch.save({
            "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
            "fingerprint": env.fingerprint(), "layout": layout, "architecture": architecture(model),
            "model": model.state_dict(), "optimizer": optimizer_state,
        }, checkpoint)
        restored, loaded = load(checkpoint, target)
        assert restored.pooling == model.pooling
        assert loaded["_optimizer_compatible"]
        critic_state = copy.deepcopy(optimizer_state)
        assert repartition_optimizer(critic_state, group_indices, 2)
        critic_optimizer = torch.optim.Adam(restored.critic.parameters())
        critic_optimizer.load_state_dict(critic_state)
        assert len(critic_optimizer.state) == len(tuple(restored.critic.parameters()))
        incompatible = torch.load(checkpoint, weights_only=False)
        incompatible["architecture"]["position_caps"]["enemy"] = 31
        torch.save(incompatible, checkpoint)
        try:
            load(checkpoint, target)
            raise AssertionError("accepted incompatible checkpoint architecture")
        except ValueError:
            pass
        compact = Agent(compact_layout, 64, 2, 4, 128).eval()
        compact.critic.weight.data.normal_(std=.1)
        env.set_potential_weights(compact.potential_weights)
        compact_observation = env.observe_tokens()
        compact_path = Path(directory) / "compact.bin"
        export_value_model(compact_path, compact, env.fingerprint(), 1, 0)
        with torch.no_grad():
            compact_python = critic_value(predict(
                compact, tensors(compact_observation, target, compact), "fp32",
            )[1]) + torch.as_tensor(compact_observation[4])
            compact_python = compact_python.sigmoid().numpy()
        compact_rust = np.asarray(env.rust_values(str(compact_path)))
        assert np.max(np.abs(compact_python - compact_rust)) < 1e-5
        compact_checkpoint = Path(directory) / "compact.pt"
        compact_optimizer = torch.optim.Adam(compact.parameters())
        torch.save({
            "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
            "fingerprint": env.fingerprint(), "layout": compact_layout,
            "architecture": architecture(compact), "model": compact.state_dict(),
            "optimizer": compact_optimizer.state_dict(),
        }, compact_checkpoint)
        restored, loaded = load(compact_checkpoint, target)
        assert (restored.model_version, restored.width, restored.layers,
                restored.heads, restored.feedforward) == (MODEL_VERSION, 64, 2, 4, 128)
        assert loaded["model_version"] == MODEL_VERSION
        incompatible = torch.load(checkpoint, weights_only=False)
        incompatible["architecture"]["position_caps"]["enemy"] = 32
        incompatible["model_version"] = MODEL_VERSION - 1
        torch.save(incompatible, checkpoint)
        try:
            load(checkpoint, target)
            raise AssertionError("accepted obsolete checkpoint")
        except ValueError:
            pass
    assert np.max(np.abs(python - rust)) <= 1e-5
    train_model = Agent(layout, pooling=mixed).train()
    inputs = tensors(combat, target, train_model)
    policy, critic = predict(train_model, inputs, "fp32")
    (policy.sum() + critic.sum()).backward()
    assert all(parameter.grad is None or parameter.grad.isfinite().all()
               for parameter in train_model.parameters())
    accelerator = device()
    if accelerator.type in ("mps", "cuda"):
        accelerated = Agent(layout, pooling=mixed).to(accelerator).train()
        inputs = tensors(combat, accelerator, accelerated)
        policy, critic = predict(accelerated, inputs, "bf16")
        (policy.sum() + critic.sum()).backward()
        assert all(parameter.grad is None or parameter.grad.isfinite().all()
                   for parameter in accelerated.parameters())
    print("probe ok")
