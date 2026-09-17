import atexit
import fcntl
import gzip
import http.server
import json
import logging
import math
import os
import shutil
import sys
import threading
import time
from pathlib import Path
from urllib.parse import parse_qs, quote, urlparse

import numpy as np

import sts2_sim
from model import *
_EVENT_FD = None
_EVENT_ROLE = None
_TRAINER_SESSION = None
_RUN_ID = None
_EVENT_PATH = None
_LOG_OUTPUT = None
_EVENT_LOCK = threading.Lock()
_LOG_CAPTURES = []
_CONSOLE_FDS = {}
_CONSOLE_STREAM = None
_CONSOLE_TTY = False
_LOG_ACTIVE = False
_LOG_STOP = b"\0spirefysh-log-stop\0"
_LOG_COMPRESSION_BYTES = 16 * 1024 * 1024
_LOGGER = logging.getLogger("spirefysh.training")
_LOGGER.propagate = False


def emit_event(value, level=None, console=True, role=None, event_time=None):
    event = value.get("event", "event")
    level = level or (
        "ERROR" if event.endswith("_error") else
        "WARNING" if event == "sampler_restart" else
        "DEBUG" if event in (
            "actor_loaded", "actor_published", "collect", "dataset_pruned",
            "expert_pruned", "heartbeat", "packet", "sample_packet",
            "sampler_start", "sampler_stop", "training_batch",
        ) else "INFO"
    )
    reserved = {"event_schema", "time", "level", "run_id", "session_id", "role", "pid", "thread"}
    overlap = reserved & value.keys()
    if overlap:
        raise ValueError(f"reserved event fields: {', '.join(sorted(overlap))}")
    record = {
        "event_schema": 2, "time": time.time() if event_time is None else event_time,
        "level": str(level).lower(),
        "run_id": _RUN_ID, "session_id": _TRAINER_SESSION,
        "role": role or _EVENT_ROLE,
        "pid": os.getpid(), "thread": threading.get_native_id(),
    } | value
    line = (json.dumps(record, separators=(",", ":")) + "\n").encode()
    if _EVENT_FD is None:
        sys.__stdout__.write(line.decode()); sys.__stdout__.flush()
        return None
    with _EVENT_LOCK:
        fcntl.flock(_EVENT_FD, fcntl.LOCK_EX)
        try:
            pending = memoryview(line)
            while pending:
                pending = pending[os.write(_EVENT_FD, pending):]
            end = os.lseek(_EVENT_FD, 0, os.SEEK_END)
        finally:
            fcntl.flock(_EVENT_FD, fcntl.LOCK_UN)
    if console and _CONSOLE_FDS and not (record["role"] or "").startswith("sampler-"):
        fields = {key: item for key, item in value.items()
                  if key not in ("event", "metrics", "pipeline", "result", "training", "time")}
        fields.update({key: item for key, item in value.get("metrics", {}).items()
                       if isinstance(item, (bool, int, float, str))})
        message = event + " " + " ".join(
            f"{key}={json.dumps(item, separators=(',', ':'))}" for key, item in fields.items()
        )
        symbol = {"DEBUG": "·", "INFO": "●", "WARNING": "▲", "ERROR": "✖"}.get(
            str(level).upper(), "●"
        )
        _LOGGER.log(getattr(logging, str(level).upper()),
                    f"{symbol} {message}" if _CONSOLE_TTY else message)
    return end


def sync_events():
    os.fsync(_EVENT_FD)


def _open_event_log(path, mode="rb"):
    return gzip.open(path, mode) if path.suffix == ".gz" else path.open(mode)


def _event_log_size(path):
    with _open_event_log(path) as source:
        source.seek(0, os.SEEK_END)
        return source.tell()


def _compress_event_log(output, path):
    if path.stat().st_size < _LOG_COMPRESSION_BYTES:
        return
    compressed = path.with_suffix(path.suffix + ".gz")
    temporary = compressed.with_suffix(compressed.suffix + ".tmp")
    try:
        with path.open("rb") as source, gzip.open(temporary, "wb") as target:
            shutil.copyfileobj(source, target)
        temporary.replace(compressed)
        relative = str(compressed.relative_to(output))
        original = str(path.relative_to(output))
        manifest_path = output / "run.json"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text())
            for session in manifest.get("sessions", ()):
                if session.get("log") == original:
                    session["log"] = relative
            atomic_json(manifest_path, manifest)
        for name in ("initial.json", "latest.json"):
            metadata_path = output / name
            if metadata_path.exists():
                metadata = json.loads(metadata_path.read_text())
                if metadata.get("event_log") == original:
                    metadata["event_log"] = relative
                    atomic_json(metadata_path, metadata)
        path.unlink()
    except (OSError, ValueError, json.JSONDecodeError):
        temporary.unlink(missing_ok=True)
        raise


def _console_handler(stream, tty, level):
    if tty:
        try:
            from rich.console import Console
            from rich.logging import RichHandler
        except ImportError:
            class ColorFormatter(logging.Formatter):
                def format(self, record):
                    colors = {logging.DEBUG: "36", logging.INFO: "32",
                              logging.WARNING: "33", logging.ERROR: "31"}
                    return f"\33[{colors.get(record.levelno, '37')}m{super().format(record)}\33[0m"
            handler = logging.StreamHandler(stream)
            handler.setFormatter(ColorFormatter(
                "%(asctime)s %(levelname)-8s %(message)s", "%H:%M:%S",
            ))
        else:
            handler = RichHandler(
                console=Console(
                    file=stream, force_terminal=True,
                    color_system="standard", no_color=False,
                ),
                rich_tracebacks=True, show_path=False,
            )
            handler.setFormatter(logging.Formatter("%(message)s"))
    else:
        handler = logging.StreamHandler(stream)
        handler.setFormatter(logging.Formatter(
            "%(asctime)s %(levelname)s %(message)s", "%Y-%m-%dT%H:%M:%S",
        ))
    handler.setLevel(level)
    return handler


def configure_logging(output, role, level="INFO", trainer_session=None, run_id=None,
                      event_log=None, first_event=None):
    global _EVENT_FD, _EVENT_ROLE, _TRAINER_SESSION, _RUN_ID, _EVENT_PATH
    global _CONSOLE_STREAM, _CONSOLE_TTY, _LOG_ACTIVE, _LOG_OUTPUT
    if _LOG_ACTIVE:
        return
    level = str(level).upper()
    if level not in logging.getLevelNamesMapping():
        raise ValueError(f"invalid log level {level}")
    _EVENT_PATH = Path(event_log) if event_log else Path(output) / "events.jsonl"
    _LOG_OUTPUT = Path(output)
    _EVENT_PATH.parent.mkdir(parents=True, exist_ok=True)
    _EVENT_FD = os.open(_EVENT_PATH, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    _EVENT_ROLE = role
    _TRAINER_SESSION = trainer_session
    _RUN_ID = run_id
    _LOG_ACTIVE = True
    def capture(fd, name, severity):
        saved = os.dup(fd)
        read_fd, write_fd = os.pipe()
        os.dup2(write_fd, fd); os.close(write_fd)

        def forward():
            with os.fdopen(read_fd, "rb", buffering=0) as source:
                for raw in iter(source.readline, b""):
                    raw = raw.rstrip(b"\r\n")
                    if raw == _LOG_STOP:
                        break
                    os.write(saved, raw + b"\n")
                    emit_event({"event": "log", "stream": name,
                                "message": raw.decode(errors="replace")}, severity, False)

        thread = threading.Thread(target=forward, name=f"{name}-capture", daemon=True)
        thread.start(); _LOG_CAPTURES.append((fd, saved, thread)); _CONSOLE_FDS[fd] = saved

    try:
        if first_event:
            emit_event(first_event, console=False)
        _CONSOLE_TTY = sys.stdout.isatty()
        sys.stdout.flush(); sys.stderr.flush()
        capture(1, "stdout", "INFO")
        capture(2, "stderr", "WARNING")
        sys.stdout.reconfigure(line_buffering=True)
        sys.stderr.reconfigure(line_buffering=True)
        _CONSOLE_STREAM = os.fdopen(os.dup(_CONSOLE_FDS[1]), "w", buffering=1)
        _LOGGER.handlers[:] = [_console_handler(_CONSOLE_STREAM, _CONSOLE_TTY, level)]
        _LOGGER.setLevel(logging.DEBUG)
        sts2_sim.configure_logging(role, level)
        emit_event({"event": "logging_started"})
    except BaseException:
        shutdown_logging()
        raise


def shutdown_logging():
    global _EVENT_FD, _EVENT_ROLE, _TRAINER_SESSION, _RUN_ID, _EVENT_PATH
    global _CONSOLE_STREAM, _CONSOLE_TTY, _LOG_ACTIVE, _LOG_OUTPUT
    if not _LOG_ACTIVE:
        return
    sys.stdout.flush(); sys.stderr.flush()
    for fd, saved, _ in _LOG_CAPTURES:
        os.write(fd, _LOG_STOP + b"\n")
        os.dup2(saved, fd)
    for _, saved, thread in _LOG_CAPTURES:
        thread.join()
        os.close(saved)
    _LOG_CAPTURES.clear()
    _CONSOLE_FDS.clear()
    for handler in _LOGGER.handlers:
        handler.flush(); handler.close()
    _LOGGER.handlers.clear()
    if _EVENT_FD is not None:
        os.close(_EVENT_FD)
    if _EVENT_ROLE == "learner" and _EVENT_PATH is not None:
        try:
            _compress_event_log(_LOG_OUTPUT, _EVENT_PATH)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            _LOGGER.warning("log compression failed: %s", error)
    if _CONSOLE_STREAM is not None:
        _CONSOLE_STREAM.close()
    _EVENT_FD = _EVENT_ROLE = _TRAINER_SESSION = _RUN_ID = _EVENT_PATH = None
    _CONSOLE_STREAM = _LOG_OUTPUT = None
    _CONSOLE_TTY = False
    _LOG_ACTIVE = False


atexit.register(shutdown_logging)


def bands(values):
    if not values:
        return dict.fromkeys((
            "min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95", "p99",
            "max", "mean",
        ))
    points = np.quantile(values, [0, .01, .05, .1, .25, .5, .75, .9, .95, .99, 1])
    return dict(zip(("min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95", "p99", "max"), map(float, points))) | {"mean": float(np.mean(values))}


def wilson(wins, runs):
    if not runs:
        return [0.0, 1.0]
    z = 1.96
    rate = wins / runs
    center = (rate + z * z / (2 * runs)) / (1 + z * z / runs)
    radius = z * math.sqrt(
        rate * (1 - rate) / runs + z * z / (4 * runs * runs)
    ) / (1 + z * z / runs)
    return [max(0, center - radius), min(1, center + radius)]


def episode_summary(rows):
    wins = sum(row[0] for row in rows)
    floors = [row[1] for row in rows]
    boss_entries = sum(row[0] or row[1] >= 51 for row in rows)
    return {
        "episodes": len(rows), "wins": wins,
        "win_rate": wins / max(1, len(rows)), "win_rate_interval": wilson(wins, len(rows)),
        "floor_mean": float(np.mean(floors)) if floors else None, "floor_bands": bands(floors),
        "boss_entries": boss_entries,
        "boss_entry_rate": boss_entries / max(1, len(rows)),
        "boss_entry_rate_interval": wilson(boss_entries, len(rows)),
        "boss_conversion": wins / max(1, boss_entries),
        "boss_conversion_interval": wilson(wins, boss_entries),
        "caps": sum(row[2] or row[3] or row[4] for row in rows),
        "step_caps": sum(row[2] for row in rows),
        "combat_caps": sum(row[3] for row in rows),
        "empty_actions": sum(row[4] for row in rows),
    }


def immutable_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True))
    temporary.replace(path)



def seed_panel(seed, runs):
    return seed, seed + 4_000_000 + runs


def event_logs(run):
    legacy = run / "events.jsonl"
    if not legacy.exists():
        legacy = legacy.with_suffix(".jsonl.gz")
    logs = {}
    for path in (*((run / "events").glob("*.jsonl.gz")),
                 *((run / "events").glob("*.jsonl"))):
        logs[path.name.removesuffix(".gz")] = path
    return ([legacy] if legacy.exists() else []) + [logs[name] for name in sorted(logs)]


def checkpoint_origin(path, digest):
    run = path.parent
    while run != run.parent and not (run / "run.json").exists():
        run = run.parent
    if not (run / "run.json").exists():
        return None
    for metadata_path in (run / "latest.json", run / "initial.json"):
        try:
            metadata = json.loads(metadata_path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        aliases = [metadata.get("checkpoint"), metadata.get("alias")]
        if metadata_path.name == "initial.json":
            aliases.append("initial.pt")
        matches_path = any(
            value and (run / value).resolve() == path.resolve() for value in aliases
        )
        if matches_path and metadata.get("sha256") == digest and all(
            metadata.get(key) is not None for key in ("run_id", "session_id", "event_end")
        ):
            return {
                "run_id": metadata["run_id"], "session_id": metadata["session_id"],
                "event_end": metadata["event_end"], "checkpoint_sha256": digest,
                "checkpoint_step": metadata.get("step"),
                "checkpoint_weights_revision": metadata.get("revisions", {}).get(
                    "weights_revision"
                ),
                "log": metadata.get("event_log"),
            }
    matches = []
    for log in event_logs(run):
        with _open_event_log(log) as source:
            while line := source.readline():
                try:
                    row = json.loads(line)
                except (json.JSONDecodeError, UnicodeDecodeError):
                    continue
                if row.get("event") == "checkpoint" and row.get("sha256") == digest:
                    matches.append((log, source.tell(), row))
    if len(matches) != 1:
        return None
    log, end, row = matches[0]
    manifest = json.loads((run / "run.json").read_text())
    return {
        "run_id": manifest.get("run_id"),
        "session_id": row.get("session_id", row.get("trainer_session")),
        "event_end": end, "checkpoint_sha256": digest,
        "checkpoint_step": row.get("step"),
        "checkpoint_weights_revision": row.get("weights_revision"),
        "log": str(log.relative_to(run)),
    }


def lineage_segments(root, run_id, session_id, end=None, seen=()):
    if (run_id, session_id) in seen:
        raise ValueError("cyclic training lineage")
    for manifest_path in root.rglob("run.json"):
        try:
            manifest = json.loads(manifest_path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        if manifest.get("run_id") != run_id:
            continue
        session = next((row for row in manifest.get("sessions", [])
                        if row.get("id") == session_id), None)
        if session is None:
            break
        parent = session.get("parent")
        prefix = lineage_segments(
            root, parent["run_id"], parent["session_id"], parent["event_end"],
            seen + ((run_id, session_id),),
        ) if parent and parent.get("run_id") and parent.get("session_id") else []
        log = manifest_path.parent / session.get("log", "events.jsonl")
        legacy = manifest_path.parent / "events.jsonl"
        if not prefix and log != legacy and legacy.exists():
            prefix = [(legacy, None)]
        return prefix + [(log, end)]
    return []


def floor_bands(histogram):
    count = sum(histogram)
    if not count:
        return dict.fromkeys((
            "min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95",
            "p99", "max", "mean",
        ))
    values = np.repeat(np.arange(len(histogram)), histogram)
    return bands(values.tolist())


class MetricsProjector:
    means = {
        "advantage_mean": "fresh_rows",
        "policy_loss": "policy_trained_rows", "critic_loss": "critic_trained_rows",
        "critic_explained_reward_variance": "critic_trained_rows",
        "critic_floor_conditioned_explained_reward_variance": None,
        "critic_expected": "critic_trained_rows",
        "critic_win_probability": "critic_trained_rows",
        "expert_loss": "expert_rows", "ppo_head_grad": "expert_rows",
        "expert_head_grad": "expert_rows", "expert_grad_cosine": "expert_rows",
        "winning_collection_drift": "winning_replayed_rows",
        "search_consistency_loss": None,
        "entropy": "policy_trained_rows", "entropy_weight": "policy_trained_rows",
        "pre_kl": "fresh_rows", "post_kl": "policy_trained_rows",
        "clip_fraction": "policy_trained_rows", "gradient_norm": None,
        "gradient_clipped": None,
        "policy_lag_mean": "fresh_rows", "policy_lag_p95": "fresh_rows",
        "unpack_seconds": None, "forward_seconds": None, "backward_seconds": None,
        "total_seconds": None,
    }

    def __init__(self, root, run, manifest, session_id):
        self.root, self.run, self.manifest, self.session_id = root, run, manifest, session_id
        self.width = manifest.get("telemetry_window_decisions", 32_768)
        self.closed, self.current, self.promotions = [], None, []
        self.status, self.last_time, self.invalid = "running", None, 0
        self.cursor = 0
        segments = lineage_segments(root, manifest["run_id"], session_id)
        if not segments:
            segments = [(run / manifest["sessions"][-1]["log"], None)]
        for index, (path, end) in enumerate(segments):
            self.read(path, end)
            if index + 1 == len(segments):
                size = _event_log_size(path)
                self.path, self.cursor = path, min(size, end or size)

    def window(self, event):
        step, stage = int(event.get("step", 0)), int(event.get("stage", 0))
        key = step // self.width, stage
        if self.current and self.current["_key"] != key:
            self.closed.append(self.render(self.current))
            self.current = None
        if self.current is None:
            self.current = {
                "_key": key, "_start_step": step,
                "_start_seconds": event.get("training_elapsed_seconds"),
                "step": step, "stage": stage, "_written": event.get("time"),
                "weights_revision": event.get("weights_revision", 0),
                "policy_revision": event.get("policy_revision", 0),
                "_sum": {}, "_weight": {}, "_floors": [0] * 53,
                "_floor_explained_sum": [0.] * (MAX_PROGRESS + 1),
                "_floor_explained_weight": [0.] * (MAX_PROGRESS + 1),
                "_floor_explained_rows": [0] * (MAX_PROGRESS + 1),
                "_policy_lag_counts": [], "_policy_lag_max": None, "_totals": {},
                "_advantage_square_sum": 0., "_advantage_rows": 0,
                "_characters": [[0, 0] for _ in range(5)],
                "_character_floors": [[0] * 53 for _ in range(5)],
                "_character_caps": [[0, 0, 0] for _ in range(5)],
                "_caps": [0, 0, 0], "_outcomes": {}, "_commits": {},
                "_samples": [[] for _ in range(5)], "_sampled": 0,
                "_trained": 0, "_critic_trained": 0, "_discarded": 0, "_batches": 0,
                "_optimizer_steps": [],
                "_collect_seconds": 0., "_update_seconds": 0.,
                "_episode_length": 0, "_episode_length_max": 0,
                "_episode_seconds": 0., "_episode_seconds_max": 0.,
                "_policy_episodes": 0, "_policy_span": 0, "_policy_span_max": 0,
                "_arrival_lag": 0, "_arrival_lag_max": 0,
                "_attempted": 0, "_admitted": 0, "_forced": 0,
                "_budget_excess": 0, "_capacity_dropped": 0,
                "_incomplete": 0, "_ratio_rejected": 0, "_stale": 0,
                "_retired": 0, "_pre_kl_rejected": 0, "_post_kl_rejected": 0,
            }
        self.current["step"] = max(self.current["step"], step)
        self.current["_written"] = event.get("time", self.current["_written"])
        self.current["weights_revision"] = event.get(
            "weights_revision", self.current["weights_revision"]
        )
        self.current["policy_revision"] = event.get(
            "policy_revision", self.current["policy_revision"]
        )
        elapsed = event.get("training_elapsed_seconds")
        if elapsed is not None:
            if self.current["_start_seconds"] is None:
                self.current["_start_seconds"] = elapsed
            self.current["_last_seconds"] = elapsed
        return self.current

    def fold_batch(self, event):
        window = self.window(event); window["_batches"] += 1
        window["_trained"] += event.get("policy_trained_rows", 0)
        window["_critic_trained"] += event.get("critic_trained_rows", 0)
        window["_attempted"] += event.get("attempted_rows", 0)
        window["_ratio_rejected"] += event.get("ratio_rejected_rows", 0)
        window["_retired"] += event.get("retired_rows", 0)
        window["_update_seconds"] += event.get("total_seconds", 0.)
        for source, target in (
            ("expert_rows", "expert_rows_used"),
            ("replay_rows", "winning_replay_candidates"),
            ("winning_replayed_rows", "winning_replayed"),
            ("winning_rejected_rows", "winning_rejected_kl"),
        ):
            value = event.get(source, 0)
            if value:
                window["_totals"][target] = window["_totals"].get(target, 0) + value
        outcome, commit = event.get("policy_outcome"), event.get("commit_kind")
        duration = event.get("total_seconds", 0.)
        if commit and commit != "none" and duration > 0:
            window["_optimizer_steps"].append({
                "step": event.get("step", 0),
                "weights_revision": event.get("weights_revision", 0),
                "seconds": event.get("training_elapsed_seconds", 0),
                "optimizer_steps_per_second": 1 / duration,
                "used_rows_per_second": event.get("critic_trained_rows", 0) / duration,
                "total_seconds": duration,
            })
        if outcome == "pre_kl_rejected":
            window["_pre_kl_rejected"] += event.get("fresh_rows", 0)
        elif outcome == "post_kl_rejected":
            window["_post_kl_rejected"] += event.get("fresh_rows", 0)
        if outcome:
            window["_outcomes"][outcome] = window["_outcomes"].get(outcome, 0) + 1
        if commit:
            window["_commits"][commit] = window["_commits"].get(commit, 0) + 1
        for metric, denominator in self.means.items():
            value = event.get(metric)
            weight = event.get(denominator, 1) if denominator else 1
            if value is None or not weight:
                continue
            window["_sum"][metric] = window["_sum"].get(metric, 0.) + value * weight
            window["_weight"][metric] = window["_weight"].get(metric, 0) + weight
        if event.get("advantage_mean") is not None \
                and event.get("advantage_stddev") is not None:
            rows = event.get("fresh_rows", 0)
            window["_advantage_square_sum"] += rows * (
                event["advantage_stddev"] ** 2 + event["advantage_mean"] ** 2
            )
            window["_advantage_rows"] += rows
        counts = event.get("policy_lag_counts", ())
        if counts:
            counts = counts.items() if isinstance(counts, dict) else enumerate(counts)
            counts = [(int(lag), count) for lag, count in counts]
            window["_policy_lag_counts"] += [0] * (
                max(lag for lag, _ in counts) + 1 - len(window["_policy_lag_counts"])
            )
            for lag, count in counts:
                window["_policy_lag_counts"][lag] += count
        if event.get("policy_lag_max") is not None:
            window["_policy_lag_max"] = max(
                event["policy_lag_max"], window["_policy_lag_max"] or 0,
            )
        for floor, metric in event.get(
                "critic_explained_reward_variance_by_floor", {}).items():
            floor, rows = int(floor), metric["rows"]
            weight = rows * metric.get("target_variance", 1)
            window["_floor_explained_sum"][floor] += metric["value"] * weight
            window["_floor_explained_weight"][floor] += weight
            window["_floor_explained_rows"][floor] += rows

    def fold_packet(self, event, event_end):
        window = self.window(event)
        window["_sampled"] += event.get("packet_sampled_decisions", 0)
        window["_discarded"] += event.get("discarded_decisions", 0)
        window["_admitted"] += event.get("admitted_rows", 0)
        window["_forced"] += event.get("forced_rows", 0)
        window["_budget_excess"] += event.get("budget_excess_rows", 0)
        window["_capacity_dropped"] += event.get("capacity_dropped_rows", 0)
        window["_collect_seconds"] += event.get("collect_seconds", 0.)
        window["_caps"][2] += event.get("orphan_empty_actions", 0)
        for source, target in (
            ("trajectory_rows", "trajectory_rows"),
            ("expert_rows", "expert_rows_collected"),
            ("winning_candidates", "winning_candidates"),
            ("winning_admitted", "winning_admitted"),
            ("winning_episodes", "winning_episodes"),
            ("winning_skipped", "winning_skipped"),
            ("winning_forced_skipped", "winning_forced_skipped"),
            ("queue_full_waits", "sample_queue_full_waits"),
        ):
            value = event.get(source, 0)
            if value:
                window["_totals"][target] = window["_totals"].get(target, 0) + value
        for group, prefix in ((event.get("cache", {}), ""), (event.get("mcts", {}), "mcts_")):
            for metric, value in group.items():
                if value:
                    key = prefix + metric
                    window["_totals"][key] = window["_totals"].get(key, 0) + value
        if event.get("queue_delay_seconds") is not None:
            window["_totals"]["sample_queue_delay_seconds"] = \
                window["_totals"].get("sample_queue_delay_seconds", 0.) \
                + event["queue_delay_seconds"]
            window["_totals"]["sample_queue_packets"] = \
                window["_totals"].get("sample_queue_packets", 0) + 1
        if event.get("queue_put_seconds"):
            window["_totals"]["sample_queue_put_seconds"] = \
                window["_totals"].get("sample_queue_put_seconds", 0.) \
                + event["queue_put_seconds"]
        for index, episode in enumerate(event.get("episodes", ())):
            character = int(episode["character"])
            floor = max(0, min(52, int(episode["floor"])))
            window["_floors"][floor] += 1
            window["_characters"][character][0] += 1
            window["_characters"][character][1] += int(episode["won"])
            window["_character_floors"][character][floor] += 1
            window["_caps"][0] += int(episode.get("step_cap", False))
            window["_caps"][1] += int(episode.get("combat_cap", False))
            window["_caps"][2] += int(episode.get("empty_actions", False))
            for target, key in enumerate(("step_cap", "combat_cap", "empty_actions")):
                window["_character_caps"][character][target] += int(episode.get(key, False))
            length = episode.get("length", 0)
            seconds = episode.get("completion_seconds", 0.)
            window["_episode_length"] += length
            window["_episode_seconds"] += seconds
            window["_episode_length_max"] = max(window["_episode_length_max"], length)
            window["_episode_seconds_max"] = max(window["_episode_seconds_max"], seconds)
            if "policy_revision_min" in episode:
                span = episode.get("policy_revision_max", 0) - episode["policy_revision_min"]
                lag = max(0, event.get("policy_revision", 0) - episode["policy_revision_min"])
                window["_policy_span"] += span
                window["_policy_span_max"] = max(window["_policy_span_max"], span)
                window["_arrival_lag"] += lag
                window["_arrival_lag_max"] = max(window["_arrival_lag_max"], lag)
                window["_policy_episodes"] += 1
            priority = hashlib.sha256(
                f"{event.get('run_id')}:{event.get('session_id')}:{event_end}:{index}".encode()
            ).digest()[:8]
            samples = window["_samples"][character]
            samples.append((priority, episode.get("iteration", 0), floor, character))
            samples.sort()
            del samples[8:]

    def fold_heartbeat(self, event):
        window = self.window(event)
        for source, target in (
            ("dataset_rows", "dataset_rows"), ("queue_depth", "sample_queue_depth"),
            ("expert_rows", "expert_buffer_rows"), ("winning_rows", "winning_reservoir"),
            ("accelerator_allocated_bytes", "accelerator_allocated_bytes"),
            ("accelerator_driver_allocated_bytes", "accelerator_driver_allocated_bytes"),
            ("queue_capacity", "sample_queue_capacity"),
            ("sampler_policy_revision", "sampler_policy_revision"),
            ("dataset_priority_mean", "dataset_priority_mean"),
            ("dataset_priority_max", "dataset_priority_max"),
            ("watchdog_dropped_steps", "watchdog_dropped_steps"),
        ):
            if source in event:
                window[target] = event[source]
        window["dataset_peak"] = max(
            window.get("dataset_peak", 0), event.get("dataset_peak_rows", 0),
        )

    def fold(self, event, event_end=0):
        self.last_time = event.get("time", self.last_time)
        kind = event.get("event")
        fact = event.get("event_schema") == 2
        if kind == "session_start" or kind == "start" and event.get("role") in (None, "learner"):
            self.status = "running"
            parent_step = event.get("parent_checkpoint_step")
            if parent_step is not None:
                self.closed = [row for row in self.closed if row["step"] <= parent_step]
                if self.current and self.current["step"] > parent_step:
                    self.current = None
            if kind == "session_start":
                self.window(event)
        elif fact and kind == "actor_published":
            window = self.window(event)
            if window["_start_seconds"] == 0 and window["_start_step"] \
                    == event.get("step", 0) != 0:
                window["_start_seconds"] = event.get("training_elapsed_seconds", 0)
        elif fact and kind == "training_batch":
            self.fold_batch(event)
        elif fact and kind == "sample_packet":
            self.fold_packet(event, event_end)
        elif fact and kind == "dataset_pruned":
            window = self.current if self.current and event.get(
                "training_elapsed_seconds"
            ) is None else self.window(event)
            window["_stale"] += event.get("stale_rows", 0)
            window["_incomplete"] += event.get("incomplete_rows", 0)
        elif fact and kind == "heartbeat":
            self.fold_heartbeat(event)
        elif fact and kind == "checkpoint":
            self.window(event)
        elif kind == "promotion":
            self.promotions.append(event | {"_written": event.get("time")})
        elif kind == "report" and event.get("event_schema") != 2:
            row = dict(event); row["_written"] = event.get("time")
            row["_session"] = f"{event.get('run_id', 'legacy')}:" \
                f"{event.get('trainer_session', 0)}:{event.get('sampler_session', 0)}"
            self.closed = [saved for saved in self.closed if saved["step"] != row.get("step")]
            self.closed.append(row)
        elif kind == "session_complete":
            self.status = "completed"
        elif kind in ("session_error", "error") and event.get("role") == "learner":
            self.status = "failed"
    def render(self, window):
        metrics = {
            key: window["_sum"][key] / window["_weight"][key]
            for key in window["_sum"]
        }
        episodes = sum(row[0] for row in window["_characters"])
        wins = sum(row[1] for row in window["_characters"])
        elapsed = next((value for value in (
            window.get("training_elapsed_seconds"), window.get("_last_seconds")
        ) if value is not None), 0)
        start_seconds = window.get("_start_seconds")
        weight_commits = sum(value for key, value in window["_commits"].items()
                             if key != "none")
        lag_counts = window["_policy_lag_counts"]
        lags = np.repeat(np.arange(len(lag_counts)), lag_counts) if lag_counts else ()
        metrics |= {
            "weights_revision": window["weights_revision"],
            "policy_version": window["policy_revision"], "episodes": episodes,
            "wins": wins, "win_rate": wins / max(1, episodes),
            "step_caps": window["_caps"][0], "combat_caps": window["_caps"][1],
            "empty_actions": window["_caps"][2], "floor_bands": floor_bands(window["_floors"]),
            "trajectory_floors": [list(sample[1:]) for rows in window["_samples"] for sample in rows],
            "sampled_decisions": window["_sampled"],
            "policy_trained_rows": window["_trained"],
            "critic_trained_rows": window["_critic_trained"],
            "discarded_steps": window["_discarded"], "seconds": elapsed,
            "actor_decisions_per_second": window["_sampled"]
            / max(1e-9, window["_collect_seconds"]),
            "learner_decisions_per_second": window["_trained"]
            / max(1e-9, window["_update_seconds"]),
            "optimizer_steps_per_second": weight_commits
            / max(1e-9, window["_update_seconds"]),
            "used_rows_per_second": window["_critic_trained"]
            / max(1e-9, window["_update_seconds"]),
            "critic_floor_conditioned_explained_reward_variance":
                sum(window["_floor_explained_sum"])
                / sum(window["_floor_explained_weight"])
                if any(window["_floor_explained_weight"])
                else metrics.get("critic_floor_conditioned_explained_reward_variance", 0),
            "critic_explained_reward_variance_by_floor": {
                str(floor): {
                    "value": window["_floor_explained_sum"][floor]
                    / window["_floor_explained_weight"][floor]
                    if window["_floor_explained_weight"][floor] else 0.,
                    "target_variance": window["_floor_explained_weight"][floor] / rows,
                    "rows": rows,
                }
                for floor, rows in enumerate(window["_floor_explained_rows"]) if rows
            },
            "optimizer_steps": window["_optimizer_steps"],
            "trajectory_length_mean": window["_episode_length"] / max(1, episodes),
            "trajectory_length_max": window["_episode_length_max"],
            "trajectory_completion_seconds_mean": window["_episode_seconds"] / max(1, episodes),
            "trajectory_completion_seconds_max": window["_episode_seconds_max"],
            "dataset_attempted": window["_attempted"],
            "dataset_admitted": window["_admitted"],
            "dataset_forced_dropped": window["_forced"],
            "dataset_budget_dropped": window["_budget_excess"],
            "dataset_capacity_dropped": window["_capacity_dropped"],
            "dataset_incomplete_dropped": window["_incomplete"],
            "dataset_rollout_dropped": max(
                0, window["_discarded"] - window["_budget_excess"]
            ),
            "dataset_ratio_dropped": window["_ratio_rejected"],
            "dataset_kl_dropped": window["_pre_kl_rejected"],
            "dataset_post_kl_dropped": window["_post_kl_rejected"],
            "dataset_retired": window["_retired"],
            "row_utilization": max(window["_trained"], window["_critic_trained"])
            / max(1, window["_attempted"]),
            "policy_row_utilization": window["_trained"] / max(1, window["_attempted"]),
            "update_attempts": window["_batches"],
            "weight_commits": weight_commits,
            "policy_commits": window["_commits"].get("full", 0),
            **{f"{key}_updates": value for key, value in window["_outcomes"].items()},
            "decisions_per_second": (
                (window["step"] - window["_start_step"]) / max(1e-9, elapsed - start_seconds)
                if start_seconds is not None and elapsed >= start_seconds else 0
            ),
            **window["_totals"],
        }
        if window["_stale"]:
            metrics["dataset_stale_dropped"] = window["_stale"]
        if window["_policy_episodes"]:
            metrics |= {
                "trajectory_policy_span_mean":
                    window["_policy_span"] / window["_policy_episodes"],
                "trajectory_policy_span_max": window["_policy_span_max"],
                "trajectory_arrival_lag_mean":
                    window["_arrival_lag"] / window["_policy_episodes"],
                "trajectory_arrival_lag_max": window["_arrival_lag_max"],
            }
        if window["_advantage_rows"]:
            metrics["advantage_stddev"] = math.sqrt(max(
                0., window["_advantage_square_sum"] / window["_advantage_rows"]
                - metrics["advantage_mean"] ** 2,
            ))
        if len(lags):
            metrics["policy_lag_mean"] = float(np.mean(lags))
            metrics["policy_lag_p95"] = float(np.quantile(lags, .95))
            metrics["policy_lag_max"] = int(lags[-1])
        elif window["_policy_lag_max"] is not None:
            metrics["policy_lag_max"] = window["_policy_lag_max"]
        cache = metrics.get("card_hits", 0) + metrics.get("card_misses", 0)
        if cache:
            metrics["card_cache_hit_rate"] = metrics.get("card_hits", 0) / cache
        cache = metrics.get("graph_hits", 0) + metrics.get("graph_misses", 0)
        if cache:
            metrics["graph_cache_hit_rate"] = metrics.get("graph_hits", 0) / cache
        packets = metrics.get("sample_queue_packets", 0)
        if packets:
            metrics["sample_queue_delay_mean"] = \
                metrics["sample_queue_delay_seconds"] / packets
        roots = metrics.get("mcts_roots", 0)
        seconds = metrics.get("mcts_seconds", 0)
        if roots:
            metrics["mcts_simulations_per_root"] = metrics.get("mcts_simulations", 0) / roots
            metrics["mcts_targets_per_root"] = metrics.get("mcts_targets", 0) / roots
            metrics["mcts_roots_per_decision"] = roots / max(1, window["_sampled"])
        turns = metrics.get("mcts_turn_starts", 0)
        if turns:
            metrics["mcts_root_fraction"] = roots / turns
        batches = metrics.get("mcts_batches", 0)
        if batches:
            metrics["mcts_leaf_batch_mean"] = metrics.get("mcts_leaves", 0) / batches
        if seconds:
            metrics["mcts_simulations_per_second"] = \
                metrics.get("mcts_simulations", 0) / seconds
            for phase in ("simulate", "encode", "inference", "backup", "rollout"):
                metrics[f"mcts_{phase}_fraction"] = \
                    metrics.get(f"mcts_{phase}_seconds", 0) / seconds
        for key in ("dataset_rows", "dataset_peak", "dataset_priority_mean",
                    "dataset_priority_max", "sample_queue_depth", "sample_queue_capacity",
                    "sampler_policy_revision", "watchdog_dropped_steps", "expert_buffer_rows",
                    "winning_reservoir", "accelerator_allocated_bytes",
                    "accelerator_driver_allocated_bytes"):
            if key in window:
                metrics[key] = window[key]
        metrics["characters"] = [
            {
                "character": character, "episodes": count, "wins": wins,
                "win_rate": wins / max(1, count),
                "floor_bands": floor_bands(window["_character_floors"][character]),
                "step_caps": window["_character_caps"][character][0],
                "combat_caps": window["_character_caps"][character][1],
                "empty_actions": window["_character_caps"][character][2],
            }
            for character, (count, wins) in enumerate(window["_characters"])
        ]
        stages = self.manifest.get("stages", ())
        stage = stages[window["stage"]] if 0 <= window["stage"] < len(stages) else {}
        return {
            "step": window["step"], "stage": {"index": window["stage"], **stage},
            "metrics": metrics, "_written": window["_written"],
            "_session": f"{self.manifest['run_id']}:{self.session_id}",
        }

    def read(self, path, end=None, start=0):
        try:
            source = _open_event_log(path)
        except OSError:
            return start
        with source:
            source.seek(start)
            position = start
            while end is None or source.tell() < end:
                line = source.readline()
                if not line or not line.endswith(b"\n"):
                    break
                position = source.tell()
                if end is not None and position > end:
                    break
                try:
                    event = json.loads(line)
                except (json.JSONDecodeError, UnicodeDecodeError):
                    self.invalid += 1
                    continue
                self.fold(event, position)
            return position

    def value(self):
        current = self.render(self.current) | {"_open": self.status == "running"} \
            if self.current else None
        reports = self.closed + ([current] if current else [])
        return {
            "schema": 2, "version": self.manifest["model_version"],
            "run_id": self.manifest["run_id"], "lineage_id": self.manifest["lineage_id"],
            "leaf_session_id": self.session_id, "revision": self.cursor,
            "status": self.status, "last_event_time": self.last_time,
            "first_retained_step": reports[0]["step"] if reports else None,
            "invalid_events": self.invalid, "reports": reports,
            "promotions": self.promotions, "cursor": self.cursor,
        }


def logged_history(run, manifest):
    reports, promotions = {}, {}
    sessions = manifest.get("sessions", [])
    session = parent_step = None
    starts = 0
    saw_report = saw_promotion = False
    for path in event_logs(run):
        try:
            lines = _open_event_log(path, "rt")
        except OSError:
            continue
        with lines:
            for line in lines:
                try:
                    row = json.loads(line)
                except (json.JSONDecodeError, TypeError):
                    continue
                event = row.get("event")
                if event in ("start", "session_start") and row.get("role") in (None, "learner"):
                    saved = sessions[starts] if starts < len(sessions) else {}
                    starts += 1
                    parent_step = row.get("parent_checkpoint_step", saved.get("step"))
                    session = row.get(
                        "session_id", row.get("trainer_session", saved.get("id", starts))
                    )
                    if (reports or promotions) and parent_step is not None:
                        reports = {step: report for step, report in reports.items()
                                   if step <= int(parent_step)}
                        promotions = {step: promotion for step, promotion in promotions.items()
                                      if step <= int(parent_step)}
                elif event == "report":
                    saw_report = True
                    try:
                        row["_written"] = row.get("time")
                        row["_trainer_session"] = row.get(
                            "session_id", row.get("trainer_session", session)
                        )
                        reports[int(row["step"])] = row
                    except (KeyError, TypeError, ValueError):
                        pass
                elif event == "promotion":
                    saw_promotion = True
                    try:
                        row["_written"] = row.get("time")
                        promotions[int(row["step"])] = row
                    except (KeyError, TypeError, ValueError):
                        pass
    return reports, promotions, session, parent_step, saw_report, saw_promotion


def report_version(report, manifest):
    marker = report.get("description", "").partition("Continuous V")[2].partition(" ")[0]
    return int(marker) if marker.isdigit() else manifest.get(
        "model_version", manifest.get("version", 0)
    )


def _cached_projection(root, run, manifest, session_id):
    segments = lineage_segments(root, manifest["run_id"], session_id)
    if not segments:
        segments = [(run / manifest["sessions"][-1]["log"], None)]
    signature = {
        "version": 1,
        "window": manifest.get("telemetry_window_decisions", 32_768),
        "manifest": [(run / "run.json").stat().st_size,
                     (run / "run.json").stat().st_mtime_ns],
        "logs": [
            [str(path.resolve()), path.stat().st_size, path.stat().st_mtime_ns, end]
            for path, end in segments
        ],
    }
    cache = run / ".telemetry-cache.json"
    try:
        saved = json.loads(cache.read_text())
        if saved["signature"] == signature:
            return saved["projection"]
    except (OSError, KeyError, json.JSONDecodeError):
        pass
    projection = MetricsProjector(root, run, manifest, session_id).value()
    atomic_json(cache, {"signature": signature, "projection": projection})
    return projection


def dashboard(target):
    if (target / "run.json").exists():
        target = target.parent
    runs = {}
    for path in target.rglob("run.json"):
        try:
            manifest = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        run = path.parent
        sessions = manifest.get("sessions", [])
        projected = None
        if sessions and manifest.get("schema", 1) >= 2:
            session_id = sessions[-1]["id"]
            try:
                projected = _cached_projection(target, run, manifest, session_id)
            except (OSError, ValueError):
                pass
        if projected:
            reports = {f"{row['step']}:{index}": row
                       for index, row in enumerate(projected["reports"])}
            promotions = dict(enumerate(projected["promotions"]))
            trainer_session = projected["leaf_session_id"]
            parent_step = sessions[-1].get("step")
            logged_reports, logged_promotions = bool(reports), bool(promotions)
        else:
            reports, promotions, trainer_session, parent_step, logged_reports, logged_promotions = \
                logged_history(run, manifest)
        if not logged_reports:
            for report in list((run / "reports").glob("*.json")) + [run / "live.json"]:
                try:
                    row = json.loads(report.read_text())
                    row["_written"] = report.stat().st_mtime
                    row["_trainer_session"] = row.get("trainer_session")
                    reports[int(row["step"])] = row
                except (OSError, json.JSONDecodeError, KeyError, ValueError):
                    pass
        if not logged_promotions:
            for promotion in (run / "promotions").glob("*.json"):
                try:
                    row = json.loads(promotion.read_text())
                    row["_written"] = promotion.stat().st_mtime
                    promotions[int(row["step"])] = row
                except (OSError, json.JSONDecodeError, KeyError, ValueError):
                    pass
        if not reports:
            archived = {}
            for suffix in ("live", "train"):
                for report in (run / "generations").glob(f"*.{suffix}.json"):
                    try:
                        row = json.loads(report.read_text())
                        archived[int(row["generation"])] = row
                    except (OSError, json.JSONDecodeError, KeyError, ValueError):
                        pass
            gate_floors = {}
            for report in (run / "generations").glob("*.gate.json"):
                try:
                    gate = json.loads(report.read_text())
                    result = (gate.get("confirmation") or {}).get("candidate") or gate.get("candidate", {})
                    gate_floors[int(gate["generation"])] = [
                        floor for character in result.get("characters", [])
                        for floor in character.get("floors", [])
                    ]
                except (OSError, json.JSONDecodeError, KeyError, ValueError):
                    pass
            offset = 0
            for row in archived.values():
                history = row.get("history", [])
                for index, point in enumerate(history):
                    step = offset + int(point.get("steps", 0))
                    metrics = dict(point)
                    if index + 1 == len(history) and gate_floors.get(int(row["generation"])):
                        metrics["terminal_floors"] = gate_floors[int(row["generation"])]
                    reports[step] = {
                        "step": step, "stage": row.get("stage", {}),
                        "description": row.get("description", "Archived training report.").replace("Generation", "Archived cycle").replace("generation", "archived cycle"),
                        "pipeline": [value.replace("generation", "cycle") for value in row.get("pipeline", [])],
                        "metrics": metrics,
                    }
                offset += max((int(point.get("steps", 0)) for point in history), default=0)
        if not reports:
            continue
        name = str(run.relative_to(target))
        try:
            best = json.loads((run / "best.json").read_text())
        except (OSError, json.JSONDecodeError):
            best = None
        report_rows = [
            {"iteration": reports[key]["metrics"].get("iteration", index),
             "step": reports[key]["step"], "stage": reports[key].get("stage", {}),
             "description": reports[key].get("description", ""),
             "pipeline": reports[key].get("pipeline", []),
             "metrics": dict(reports[key]["metrics"]), "_written": reports[key].get("_written"),
             "_open": reports[key].get("_open", False),
             "_session": reports[key].get("_session") or
                         f"{name}:{reports[key].get('_trainer_session')}:"
                         f"{reports[key].get('sampler_session')}"}
            for index, key in enumerate(sorted(
                reports, key=lambda value: (reports[value]["step"], reports[value].get("_written", 0))
            ), 1)
        ]
        for row in report_rows:
            floors = row["metrics"].get("trajectory_floors", [])
            floor_groups = [[] for _ in range(5)]
            character = 0; previous = -math.inf
            for floor in floors:
                character = min(4, max(0, int(floor[2]))) if len(floor) > 2 \
                    else min(4, character + int(floor[0] < previous))
                floor_groups[character].append(floor)
                previous = floor[0]
            row["metrics"]["trajectory_floors"] = [
                floor for group in floor_groups
                for floor in group[::max(1, math.ceil(len(group) / 8))]
            ]
            row["metrics"].pop("characters", None)
        version_groups = {}
        for row in report_rows:
            version = report_version(row, manifest)
            version_groups.setdefault(version, []).append(row)
        for version, version_reports in version_groups.items():
            for row in version_reports[1:]:
                row.pop("description", None); row.pop("pipeline", None)
            version_manifest = manifest | manifest.get("version_history", {}).get(str(version), {})
            version_manifest["model_version"] = version
            if version in COMPATIBLE_MODEL_VERSIONS:
                saved = version_manifest.get("architecture", {})
                current = Agent(
                    version_manifest["layout"],
                    *(saved.get(key, default) for key, default in zip(
                        ("width", "layers", "heads", "feedforward", "head_width"),
                        (*DEFAULT_ARCHITECTURE, DEFAULT_ARCHITECTURE[0]),
                    )),
                    pooling=saved.get("pooling"), model_version=version,
                )
                version_manifest |= {
                    "architecture": architecture(current),
                    "feature_version": FEATURE_VERSION,
                    "parameters": sum(parameter.numel() for parameter in current.parameters()),
                }
                if version == MODEL_VERSION:
                    version_manifest["change"] = CHANGE
            if len(version_groups) > 1:
                sessions = [row for row in manifest.get("sessions", []) if row["step"] <= version_reports[-1]["step"]]
                version_manifest["sessions"] = sessions[-1:] or manifest.get("sessions", [])[:1]
            low, high = version_reports[0]["step"], version_reports[-1]["step"]
            key = name if len(version_groups) == 1 else f"{name}/V{version}"
            live = f"/api?run={quote(name, safe='')}&version={version}" \
                if projected and version == manifest.get("model_version") else None
            stable = [row for row in version_reports if not row["_open"]]
            version_promotions = [row for row in promotions.values()
                                  if low <= row.get("step", low) <= high]
            runs[key] = {
                "version": version, "manifest": version_manifest,
                "source": (None if projected else
                           manifest.get("source") if len(version_groups) == 1 else None),
                "reports": version_reports,
                "promotions": version_promotions,
                "best": best if version == manifest.get("model_version") else None, "live": live,
                "trainer_session": trainer_session, "parent_checkpoint_step": parent_step,
                "run_id": manifest.get("run_id"),
                "lineage_id": manifest.get("lineage_id", manifest.get("run_id", name)),
                "live_revision": projected.get("revision", 0) if projected else 0,
                "report_count": len(stable),
                "optimizer_step_count": sum(len(row["metrics"].get("optimizer_steps", ()))
                                            for row in version_reports),
                "promotion_count": len(version_promotions),
                "live_token": f"{manifest.get('run_id')}:{trainer_session}:{version}",
                "status": projected.get("status") if projected else None,
                "last_event_time": projected.get("last_event_time") if projected else None,
            }
    def history(name, seen=()):
        row = runs[name]
        source = Path(row["source"]).resolve() if row["source"] else None
        parents = [candidate for candidate in runs if candidate not in seen + (name,)
                   and source and source.is_relative_to((target / candidate).resolve())]
        compatible = lambda candidate: runs[candidate]["version"] == row["version"] and runs[candidate]["manifest"].get("architecture") == row["manifest"].get("architecture")
        parents = [candidate for candidate in parents if compatible(candidate)]
        start = min((int(session["step"]) for session in row["manifest"].get("sessions", []) if "step" in session), default=None)
        if not parents and source and start is not None:
            parents = [candidate for candidate in runs if candidate not in seen + (name,)
                       and compatible(candidate) and any(
                           (target / candidate / "checkpoints").glob(f"{start:012}*.pt")
                       )]
        parent = max(parents, key=lambda candidate: runs[candidate]["reports"][-1]["step"], default=None)
        inherited = [report for report in history(parent, seen + (name,)) if start is None or report["step"] <= start] if parent else []
        return list({report["step"]: report for report in inherited + row["reports"]}.values())
    histories = {name: sorted(history(name), key=lambda report: report["step"]) for name in runs}
    for name in runs:
        runs[name]["reports"] = histories[name]
        runs[name].pop("source")
    runs = dict(sorted(runs.items(), key=lambda item: (item[1]["version"], item[1]["reports"][-1]["step"], item[0])))
    data = json.dumps({name: run | {"run": name} for name, run in runs.items()}, separators=(",", ":")).replace("</", "<\\/")
    content = """<!doctype html><meta charset=utf-8><title>Spirefysh dashboard</title><script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>
body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}h1,h2{margin-bottom:6px}.controls,.legend{display:flex;align-items:center;gap:12px;flex-wrap:wrap}.controls{margin-bottom:12px}.legend{margin-bottom:22px}.legend-item{display:flex;align-items:center;gap:6px}.legend-line{width:24px;border-top:3px solid}.legend-dot{width:9px;height:9px;border-radius:50%}.metric-picker{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));margin-top:12px}.metric-picker label{padding:3px}summary{cursor:pointer;font-size:18px;font-weight:600}select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.panel{margin:22px 0;padding:16px;background:#171d28;border:1px solid #303a49;border-radius:10px}.charts .panel{margin:0}.plot{height:340px;min-width:0}.note{color:#9aa8bb;margin:0}table{border-collapse:collapse;width:100%}th,td{padding:7px;border-bottom:1px solid #303a49;text-align:left}.yes{color:#75db91}.no{color:#ee7b7b}
</style><h1 id=title>Spirefysh dashboard</h1><div class=controls><label>Lineage <select id=lineage></select></label><label>Branch <select id=version></select></label><label>Compare <select id=compare></select></label><label>X axis <select id=xaxis><option value=updates>Weights revision</option><option value=decisions selected># decisions</option><option value=time>Active training time</option></select></label><label><input id=smooth type=checkbox checked> EMA</label><label>EMA α <input id=ema type=number min=.01 max=1 step=.01 value=.2></label><label>Subsample <input id=subsample type=number min=1 step=1 value=3></label><span>Auto-refresh 2s</span><span id=status></span></div><div id=legend class=legend></div><div class=charts><section class=panel><h2>Mean advantage</h2><div id=advantage class=plot></div></section><section class=panel><h2>Optimizer steps / second</h2><p class=note>One point per committed step</p><div id=optimizer-rate class=plot></div></section><section class=panel><h2>Used rows / second</h2><p class=note>Critic-consumed rows per committed step</p><div id=row-rate class=plot></div></section><section class=panel><h2>Terminal floor</h2><div id=floor class=plot></div></section><section class=panel><h2>Ascension</h2><div id=ascension class=plot></div></section><section class=panel><h2>Bonus strength</h2><div id=bonus class=plot></div></section><section class=panel><h2>Win proportion</h2><div id=wins class=plot></div></section><section class=panel><h2>Clip fraction</h2><div id=clip class=plot></div></section><section class=panel><h2>KL</h2><div id=kl class=plot></div></section><section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section><section class=panel><h2>Policy loss</h2><div id=policy class=plot></div></section><section class=panel><h2>Critic loss</h2><div id=critic class=plot></div></section><section class=panel><h2>Optimizer step time</h2><p class=note>Seconds per committed step</p><div id=duration class=plot></div></section><section class=panel><h2>Policy lag (p95)</h2><div id=lag class=plot></div></section><section class=panel><h2>Gradient norm</h2><div id=gradient class=plot></div></section><section class=panel><h2>Dataset size</h2><div id=dataset class=plot></div></section></div><details class=panel open><summary>Metrics</summary><div id=metrics class=metric-picker></div></details><section class=panel><h2>Row outcomes</h2><div id=rows></div></section><section class=panel><h2>Promotion</h2><div id=promotion></div></section><script>const versions=""" + data + r""",lineageSelect=document.querySelector('#lineage'),versionSelect=document.querySelector('#version'),compareSelect=document.querySelector('#compare'),xaxis=document.querySelector('#xaxis'),smooth=document.querySelector('#smooth'),ema=document.querySelector('#ema'),subsample=document.querySelector('#subsample'),metricPicker=document.querySelector('#metrics'),charts=document.querySelector('.charts');
const characterNames=['Ironclad','Defect','Silent','Regent','Necrobinder'],characterColors=['#ef4444','#38bdf8','#22c55e','#f59e0b','#a78bfa'],runColors=['#6fb1ff','#f472b6'],names=Object.keys(versions),lineages=[...new Set(names.map(name=>versions[name].lineage_id||name))].sort((left,right)=>names.find(name=>(versions[name].lineage_id||name)===left).localeCompare(names.find(name=>(versions[name].lineage_id||name)===right),undefined,{numeric:true})),config={responsive:true,displaylogo:false},refreshKey='spirefysh-dashboard',saved=(()=>{try{return JSON.parse(sessionStorage.getItem(refreshKey))}catch{return null}})();let timeOrigin=0,sharedRange,syncingAxes=false,uiRevision='';const initial=saved?.followLatest?names.at(-1):names.includes(saved?.version)?saved.version:names.at(-1)||'';lineageSelect.innerHTML=lineages.map(id=>`<option value="${id}">${names.find(name=>(versions[name].lineage_id||name)===id)||id}</option>`).join('');lineageSelect.value=versions[initial]?.lineage_id||initial;function showBranches(preferred){const branches=names.filter(name=>(versions[name].lineage_id||name)===lineageSelect.value);versionSelect.innerHTML=branches.map(name=>`<option value="${name}">V${versions[name].version} · ${name}</option>`).join('');versionSelect.value=branches.includes(preferred)?preferred:branches.at(-1)||''}showBranches(initial);compareSelect.innerHTML='<option value="">None</option>'+names.map(name=>`<option value="${name}">V${versions[name].version} · ${name}</option>`).join('');compareSelect.value=names.includes(saved?.compare)?saved.compare:'';if(saved?.xaxis)xaxis.value=saved.xaxis==='iteration'?'updates':saved.xaxis;if(typeof saved?.smooth==='boolean')smooth.checked=saved.smooth;if(saved?.ema)ema.value=saved.ema;if(saved?.subsample)subsample.value=saved.subsample;
const staticMetrics={advantage_mean:'advantage',optimizer_steps_per_second:'optimizer-rate',used_rows_per_second:'row-rate',floor_bands:'floor',ascension:'ascension',bonus:'bonus',win_rate:'wins',clip_fraction:'clip',post_kl:'kl',entropy:'entropy',policy_loss:'policy',critic_loss:'critic',total_seconds:'duration',policy_lag_p95:'lag',gradient_norm:'gradient',dataset_rows:'dataset'},metricTitles={advantage_mean:'Mean advantage',optimizer_steps_per_second:'Optimizer steps / second',used_rows_per_second:'Used rows / second',floor_bands:'Terminal floor',ascension:'Ascension',bonus:'Bonus strength',win_rate:'Win proportion',clip_fraction:'Clip fraction',post_kl:'KL',entropy:'Entropy',policy_loss:'Policy loss',critic_loss:'Critic loss',total_seconds:'Optimizer step time',policy_lag_p95:'Policy lag (p95)',gradient_norm:'Gradient norm',dataset_rows:'Dataset size',critic_explained_reward_variance:'Critic explained variance'},defaults=[...Object.keys(staticMetrics),'critic_explained_reward_variance'],metricKeys=[...new Set(defaults.concat(Object.values(versions).flatMap(run=>run.reports).flatMap(report=>Object.entries(report.metrics).filter(([,value])=>typeof value==='number'&&Number.isFinite(value)).map(([key])=>key))))].sort(),metricIds=Object.fromEntries(metricKeys.filter(key=>!staticMetrics[key]).map((key,index)=>[key,`metric-${index}`]));let selectedMetrics=new Set(saved?.metrics?.filter(key=>metricKeys.includes(key))??defaults);function metricLabel(key){return metricTitles[key]||key.replaceAll('_',' ')}metricPicker.innerHTML=metricKeys.map(key=>`<label><input type=checkbox value="${key}"${selectedMetrics.has(key)?' checked':''}> ${metricLabel(key)}</label>`).join('');function updateMetricPanels(){for(const [key,id] of Object.entries(staticMetrics))document.querySelector(`#${id}`).parentElement.hidden=!selectedMetrics.has(key);for(const [key,id] of Object.entries(metricIds)){let node=document.querySelector(`#${id}`);if(selectedMetrics.has(key)&&!node){charts.insertAdjacentHTML('beforeend',`<section class=panel><h2>${metricLabel(key)}</h2><div id="${id}" class=plot></div></section>`);node=document.querySelector(`#${id}`)}if(node)node.parentElement.hidden=!selectedMetrics.has(key)}}metricPicker.onchange=()=>{selectedMetrics=new Set([...metricPicker.querySelectorAll('input:checked')].map(input=>input.value));showVersion(false)};
function monotonic(history){let step=-Infinity;return history.filter(row=>row.step>step&&(step=row.step,true))}function updateSteps(history){let offset=0,last=0,session;for(const report of history){const updates=Number(report.metrics.weights_revision??report.metrics.updates)||0;if(session!==undefined&&(report._session!==session||updates<last)){offset+=last;last=0}report._updates=offset+updates;last=Math.max(last,updates);session=report._session}}function x(report){return xaxis.value==='time'?(Number.isFinite(Number(report.metrics.seconds))?report.metrics.seconds/60:Number.isFinite(Number(report._written))?(report._written-timeOrigin)/60:0):xaxis.value==='decisions'?report.step:report._updates}function optimizerX(step){return xaxis.value==='time'?step.seconds/60:xaxis.value==='decisions'?step.step:step._updates}function series(history,key,fallback){return history.map(report=>({x:x(report),y:Number(report.metrics[key]??report.metrics[fallback])})).filter(point=>Number.isFinite(point.y))}function optimizerSeries(steps,history,key){return steps.length?steps.map(step=>({x:optimizerX(step),y:Number(step[key])})):series(history.filter(report=>(report.metrics.update_attempts??1)>0),key)}function optimizerSteps(run,reports){if(!run.optimizer_steps)run.optimizer_steps=reports.flatMap(report=>{const offset=report._updates-Number(report.metrics.weights_revision??report.metrics.updates);return(report.metrics.optimizer_steps||[]).map(step=>({...step,_updates:offset+step.weights_revision}))});return run.optimizer_steps}function speedMetrics(history){for(const report of history){const metrics=report.metrics,seconds=Number(metrics.total_seconds)*Number(metrics.update_attempts);if(!Number.isFinite(Number(metrics.optimizer_steps_per_second))&&seconds>0)metrics.optimizer_steps_per_second=Number(metrics.weight_commits)/seconds;if(!Number.isFinite(Number(metrics.used_rows_per_second)))metrics.used_rows_per_second=seconds>0?Number(metrics.critic_trained_rows)/seconds:Number(metrics.learner_decisions_per_second)}}function prepareRun(run){const all=monotonic(run.reports);updateSteps(all);const reports=all.filter(row=>!row._open),steps=optimizerSteps(run,all),last=all.at(-1),revision=Number(last?.metrics.weights_revision??last?.metrics.updates)||0,timed=all.find(row=>Number.isFinite(Number(row._written))&&Number.isFinite(Number(row.metrics.seconds)));run._updateOffset=(last?last._updates:0)-revision;run._timeOrigin=timed?timed._written-timed.metrics.seconds:0;speedMetrics(reports);return{run,reports,steps}}function rangeOf(values){let low=Infinity,high=-Infinity;for(const value of values)if(Number.isFinite(value)){low=Math.min(low,value);high=Math.max(high,value)}if(low===Infinity)return;const padding=(high-low||Math.abs(high)*.02||1)*.02;return[low-padding,high+padding]}function setFullRange(data){sharedRange=rangeOf(data.flatMap(({run,reports,steps})=>{timeOrigin=run._timeOrigin;return reports.map(x).concat(steps.map(optimizerX))}))}function clipComparison(comparison,primary){timeOrigin=primary.run._timeOrigin;const end=Math.max(...primary.reports.map(x).concat(primary.steps.map(optimizerX)).filter(Number.isFinite));timeOrigin=comparison.run._timeOrigin;return{...comparison,end,reports:comparison.reports.filter(report=>x(report)<=end),steps:comparison.steps.filter(step=>optimizerX(step)<=end)}}
function emaLine(points){const alpha=Math.max(.01,Math.min(1,Number(ema.value)||.2));let value;return points.map((point,index)=>({x:point.x,y:value=index?alpha*point.y+(1-alpha)*value:point.y}))}function sampled(points,offset=0){const factor=Math.max(1,Math.floor(Number(subsample.value)||3));return factor===1?points:points.filter((_,index)=>(offset+index)%factor===0)}
function stageTransitions(history,run){const promotions=(run?.promotions||[]).filter(row=>row.promoted);if(promotions.length)return promotions.map(promotion=>{const report=history.find(row=>row.step===promotion.step)||history.filter(row=>row.step<=promotion.step).at(-1),after=history.find(row=>row.step>promotion.step),next=run.manifest.stages?.[(promotion.stage?.index??-1)+1]||after?.stage,position=xaxis.value==='decisions'?promotion.step:report&&xaxis.value==='updates'?report._updates:Number.isFinite(Number(promotion._written))?(promotion._written-timeOrigin)/60:Number.isFinite(Number(promotion.seconds))?promotion.seconds/60:report?x(report):NaN;return{x:position,stage:next}}).filter(point=>Number.isFinite(Number(point.x)));return history.slice(1).flatMap((row,index)=>Number.isFinite(Number(row.stage?.ascension))&&Number.isFinite(Number(history[index].stage?.ascension))&&(row.stage.ascension!==history[index].stage.ascension||row.stage.bonus!==history[index].stage.bonus)?[{x:x(row),stage:row.stage}]:[])}
function stageLines(history,run){return stageTransitions(history,run).map(point=>({type:'line',xref:'x',yref:'paper',x0:point.x,x1:point.x,y0:0,y1:1,layer:'below',line:{color:'rgba(232,236,242,.38)',width:1,dash:'dash'}}))}
function layout(percent=false,range,history=[],run){const title=xaxis.value==='time'?'Wall-clock time (minutes)':xaxis.value==='decisions'?'# decisions':'Optimizer steps';return{template:'plotly_dark',uirevision:uiRevision,paper_bgcolor:'rgba(0,0,0,0)',plot_bgcolor:'rgba(0,0,0,0)',margin:{l:62,r:18,t:12,b:52},hovermode:'closest',showlegend:false,shapes:stageLines(history,run),xaxis:{title,gridcolor:'#303a49',autorange:!sharedRange,range:sharedRange},yaxis:{gridcolor:'#303a49',tickformat:percent?'.0%':undefined,range,zerolinecolor:'#8794a8'}}}function visiblePlots(){return[...document.querySelectorAll('.plot')].filter(node=>!node.parentElement.hidden)}function bindAxes(){for(const plot of visiblePlots())if(!plot._xSync&&plot.on){plot._xSync=true;plot.on('plotly_relayout',event=>{if(syncingAxes)return;const indexed=Number.isFinite(event['xaxis.range[0]'])&&Number.isFinite(event['xaxis.range[1]']),range=event['xaxis.range']||(indexed?[event['xaxis.range[0]'],event['xaxis.range[1]']]:event['xaxis.autorange']?rangeOf(visiblePlots().flatMap(node=>node.data.flatMap(trace=>trace.x||[]))):null);if(!range)return;sharedRange=range;syncingAxes=true;Promise.all(visiblePlots().filter(node=>node!==plot).map(node=>Plotly.relayout(node,{'xaxis.range':range,'xaxis.autorange':false}))).finally(()=>syncingAxes=false)})}}
function lineTraces(points,label,color){const prefix=label?`${label} · `:'',shown=sampled(points),line=emaLine(points),traces=[{x:shown.map(point=>point.x),y:shown.map(point=>point.y),mode:'lines',name:`${prefix}raw`,line:{color,width:1},opacity:.45,hovertemplate:'x %{x}<br>y %{y:.5g}<extra></extra>'}];if(smooth.checked){const visible=sampled(line);traces.push({x:visible.map(point=>point.x),y:visible.map(point=>point.y),mode:'lines',name:`${prefix}EMA α=${Number(ema.value)||.2}`,line:{color,width:3},hovertemplate:'EMA %{y:.5g}<extra></extra>'})}return{traces,seen:points.length,ema:line.at(-1)?.y}}function plot(id,points,{range,percent=false,tozero=false,history=[],run,label=''}={}){const node=document.querySelector(`#${id}`);if(node.parentElement.hidden)return;const built=lineTraces(points,label,runColors[0]);node._series=[{seen:built.seen,ema:built.ema,raw:0,smooth:smooth.checked?1:null}];const options=layout(percent,range,history,run);if(tozero)options.yaxis.rangemode='tozero';Plotly.react(node,built.traces,options,config)}function addPlot(id,points,label){const node=document.querySelector(`#${id}`);if(!node?._series||node.parentElement.hidden)return;const built=lineTraces(points,label,runColors[1]),start=node.data.length;node._series[1]={seen:built.seen,ema:built.ema,raw:start,smooth:smooth.checked?start+1:null};Plotly.addTraces(node,built.traces)}function extendPlot(id,points,index=0){if(!points.length)return;const node=document.querySelector(`#${id}`);if(!node||node.parentElement.hidden)return;const state=node._series?.[index];if(!state)return;const offset=state.seen,shown=sampled(points,offset);state.seen+=points.length;const update={x:[shown.map(point=>point.x)],y:[shown.map(point=>point.y)]},traces=[state.raw];if(state.smooth!==null){const alpha=Math.max(.01,Math.min(1,Number(ema.value)||.2)),line=points.map(point=>({x:point.x,y:state.ema=state.ema===undefined?point.y:alpha*point.y+(1-alpha)*state.ema})),visible=sampled(line,offset);update.x.push(visible.map(point=>point.x));update.y.push(visible.map(point=>point.y));traces.push(state.smooth)}if(update.x.some(values=>values.length))Plotly.extendTraces(node,update,traces)}
function trajectoryPoints(history){const points=[];for(const report of history){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:x(report),y:floor,character,updates:report._updates,step:report.step});last=iteration}}return points}function floorTraces(history,label='',secondary=false){const completed=history.filter(report=>Number(report.metrics.episodes)>0||(report.metrics.trajectory_floors?.length??0)>0),prefix=label?`${label} · `:'',rgb=secondary?'244,114,182':'111,177,255',traces=[],seen={bands:[],points:0,means:0},bands=[['min','max','min–max','.04'],['p01','p99','p1–p99','.06'],['p05','p95','p5–p95','.09'],['p10','p90','p10–p90','.13'],['p25','p75','p25–p75','.20']];for(const [low,high,name,alpha] of bands){let rows=completed.map(report=>({x:x(report),low:report.metrics.floor_bands?.[low],high:report.metrics.floor_bands?.[high]})).filter(row=>Number.isFinite(row.low)&&Number.isFinite(row.high));seen.bands.push(rows.length);rows=sampled(rows);traces.push({x:rows.map(row=>row.x),y:rows.map(row=>row.low),mode:'lines',showlegend:false,hoverinfo:'skip',line:{width:0}},{x:rows.map(row=>row.x),y:rows.map(row=>row.high),mode:'lines',name:`${prefix}${name}`,line:{width:0},fill:'tonexty',fillcolor:`rgba(${rgb},${alpha})`,hovertemplate:`${name}<br>upper %{y:.2f}<extra></extra>`})}let points=trajectoryPoints(completed);seen.points=points.length;points=sampled(points);for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}traces.push({x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.updates,point.step]),mode:'markers',name:`${prefix}trajectories`,marker:{color:points.map(point=>characterColors[point.character]),symbol:secondary?'x':'circle',size:5,opacity:.28},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>optimizer steps %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'});let means=completed.map(report=>({x:x(report),y:report.metrics.floor_bands?.mean??report.metrics.floor_mean})).filter(point=>Number.isFinite(point.y));seen.means=means.length;const line=emaLine(means);means=sampled(means);traces.push({x:means.map(point=>point.x),y:means.map(point=>point.y),mode:'lines',name:`${prefix}mean`,line:{color:runColors[secondary?1:0],width:3},hovertemplate:'mean %{y:.2f}<extra></extra>'});if(smooth.checked){const visible=sampled(line);traces.push({x:visible.map(point=>point.x),y:visible.map(point=>point.y),mode:'lines',name:`${prefix}mean EMA α=${Number(ema.value)||.2}`,line:{color:runColors[secondary?1:0],width:2,dash:'dot'},hovertemplate:'mean EMA %{y:.2f}<extra></extra>'})}return{traces,seen,ema:line.at(-1)?.y}}function rememberFloor(node,index,start,built){node._floor??=[];node._floor[index]={...built.seen,ema:built.ema,start,smooth:smooth.checked?start+12:null}}function floorPlot(history,run,label){const node=document.querySelector('#floor');if(node.parentElement.hidden)return;const built=floorTraces(history,label);rememberFloor(node,0,0,built);const options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react(node,built.traces,options,config)}function addFloor(history,label){const node=document.querySelector('#floor');if(!node._floor)return;const built=floorTraces(history,label,true),start=node.data.length;rememberFloor(node,1,start,built);Plotly.addTraces(node,built.traces)}
function stageTrace(history,key,color,run,label='',end=Infinity){let rows=history.filter(row=>Number.isFinite(Number(row.stage?.[key]))).map(row=>({x:x(row),y:row.stage[key]}));for(const transition of stageTransitions(history,run))if(Number.isFinite(Number(transition.stage?.[key])))rows.push({x:transition.x,y:transition.stage[key]});rows=rows.filter(row=>row.x<=end).sort((left,right)=>left.x-right.x);const seen=rows.length,name=key==='ascension'?'Ascension':'Bonus strength';rows=sampled(rows);return{seen,trace:{x:rows.map(row=>row.x),y:rows.map(row=>row.y),mode:'lines',name:label?`${label} · ${name}`:name,line:{color,width:3,shape:'hv'},hovertemplate:`${key==='ascension'?'ascension':'bonus'} %{y}<extra></extra>`}}}function stagePlot(id,history,key,color,run,label){const node=document.querySelector(`#${id}`);if(node.parentElement.hidden)return;const built=stageTrace(history,key,color,run,label);node._stage=[{seen:built.seen,trace:0}];const options=layout(false,undefined,history,run);options.yaxis={...options.yaxis,title:key==='ascension'?'Ascension':'Bonus strength',rangemode:'tozero',dtick:key==='ascension'?1:4};Plotly.react(node,[built.trace],options,config)}function addStage(id,history,key,run,label,end){const node=document.querySelector(`#${id}`);if(!node._stage)return;const built=stageTrace(history,key,runColors[1],run,label,end),trace=node.data.length;node._stage[1]={seen:built.seen,trace};Plotly.addTraces(node,[built.trace])}
function promotionSummary(row){if(!row)return '<p>No promotion check yet.</p>';const characters=row.result?.characters||[],rows=characters.map(item=>`<tr><td>${characterNames[item.character]??`Character ${item.character}`}</td><td>${item.wins}/${item.runs}</td><td>${(100*item.wins/item.runs).toFixed(1)}%</td><td>${Number(item.floor_mean).toFixed(2)}</td><td>${item.caps}</td></tr>`).join('');return `<p class="${row.promoted?'yes':'no'}">${row.promoted?'Promoted':'Stayed at current stage'} · threshold ${(100*row.threshold).toFixed(0)}% per character · seed ${row.seed}</p><table><thead><tr><th>Character</th><th>Wins</th><th>Rate</th><th>Mean floor</th><th>Caps</th></tr></thead><tbody>${rows}</tbody></table>`}
function rowSummary(history){const items=[['critic_trained_rows','Used by optimizer'],['discarded_steps','Sampler-discarded total'],['dataset_rollout_dropped','Rollout discard'],['dataset_budget_dropped','Over decision budget'],['dataset_forced_dropped','Forced-action excluded'],['dataset_capacity_dropped','FIFO capacity eviction'],['dataset_incomplete_dropped','Incomplete final batch'],['dataset_stale_dropped','Legacy stale eviction'],['dataset_ratio_dropped','Behavior-ratio rejected'],['dataset_kl_dropped','Policy rejected before update (critic used)'],['dataset_post_kl_dropped','Policy rejected after trial (critic used)'],['dataset_retired','Used, then retired']],projected=history.filter(row=>'update_attempts'in row.metrics),rows=projected.length?projected:history.slice(-1),latest=rows.at(-1)?.metrics||{},number=value=>Number.isFinite(value)?value.toLocaleString():'—',body=items.map(([key,label])=>{const values=rows.map(row=>Number(row.metrics[key])).filter(Number.isFinite);return `<tr><td>${label}</td><td>${number(Number(latest[key]))}</td><td>${number(values.length?values.reduce((sum,value)=>sum+value,0):NaN)}</td></tr>`}).join('');return `<table><thead><tr><th>Outcome</th><th>Latest window</th><th>Displayed total</th></tr></thead><tbody>${body}</tbody></table>`}
function saveDashboardState(){try{sessionStorage.setItem(refreshKey,JSON.stringify({version:versionSelect.value,compare:compareSelect.value,metrics:[...selectedMetrics],followLatest:versionSelect.value===names.at(-1),xaxis:xaxis.value,smooth:smooth.checked,ema:ema.value,subsample:subsample.value,scroll:[scrollX,scrollY]}))}catch{}}function restoreDashboardState(){if(saved?.scroll)scrollTo(...saved.scroll)}
function showStatus(run){const age=Date.now()/1000-(run.last_event_time||0),timeout=run.manifest.sessions?.at(-1)?.training?.sampler_timeout||120;document.querySelector('#status').textContent=run.status==='running'&&age>timeout?'stalled':run.status||''}function showLegend(primary,comparison){const runs=[primary,comparison].filter(Boolean),runItems=runs.map((item,index)=>`<span class=legend-item><i class=legend-line style="border-color:${runColors[index]}"></i>${item.run.run}</span>`).join(''),characters=characterNames.map((name,index)=>`<span class=legend-item><i class=legend-dot style="background:${characterColors[index]}"></i>${name}</span>`).join('');document.querySelector('#legend').innerHTML=`${runItems}<span class=note>${smooth.checked?'thin raw · bold EMA':'raw'} · terminal floors:</span>${characters}`}function dynamicPlots(reports,run,label,index=0){for(const [key,id] of Object.entries(metricIds))if(selectedMetrics.has(key)){const points=series(reports,key);index?addPlot(id,points,label):plot(id,points,{history:reports,run,label})}}function addComparison({run,reports,steps,end}){timeOrigin=run._timeOrigin;const label=run.run;addPlot('advantage',series(reports,'advantage_mean','mean_advantage'),label);addPlot('optimizer-rate',optimizerSeries(steps,reports,'optimizer_steps_per_second'),label);addPlot('row-rate',optimizerSeries(steps,reports,'used_rows_per_second'),label);addFloor(reports,label);addStage('ascension',reports,'ascension',run,label,end);addStage('bonus',reports,'bonus',run,label,end);addPlot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),label);addPlot('clip',series(reports,'clip_fraction'),label);addPlot('kl',series(reports,'post_kl','kl'),label);addPlot('entropy',series(reports,'entropy'),label);addPlot('policy',series(reports,'policy_loss'),label);addPlot('critic',series(reports,'critic_loss'),label);addPlot('duration',optimizerSeries(steps,reports,'total_seconds'),label);addPlot('lag',series(reports,'policy_lag_p95'),label);addPlot('gradient',series(reports,'gradient_norm'),label);addPlot('dataset',series(reports,'dataset_rows'),label);dynamicPlots(reports,run,label,1)}function showVersion(resetRange=true){updateMetricPanels();if(compareSelect.value===versionSelect.value)compareSelect.value='';for(const option of compareSelect.options)option.disabled=option.value===versionSelect.value;const primary=prepareRun(versions[versionSelect.value]),{run,reports,steps}=primary,selectedComparison=compareSelect.value&&prepareRun(versions[compareSelect.value]),comparison=selectedComparison&&clipComparison(selectedComparison,primary),label=comparison?run.run:'';uiRevision=`${versionSelect.value}:${compareSelect.value}:${xaxis.value}`;if(resetRange)setFullRange([primary]);timeOrigin=run._timeOrigin;document.querySelector('#title').textContent=`Spirefysh V${run.version} · ${run.run}${comparison?` vs ${comparison.run.run}`:''}`;showStatus(run);showLegend(primary,comparison);plot('advantage',series(reports,'advantage_mean','mean_advantage'),{history:reports,run,label});plot('optimizer-rate',optimizerSeries(steps,reports,'optimizer_steps_per_second'),{tozero:true,history:reports,run,label});plot('row-rate',optimizerSeries(steps,reports,'used_rows_per_second'),{tozero:true,history:reports,run,label});floorPlot(reports,run,label);stagePlot('ascension',reports,'ascension',runColors[0],run,label);stagePlot('bonus',reports,'bonus',runColors[0],run,label);plot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),{range:[0,1],percent:true,history:reports,run,label});plot('clip',series(reports,'clip_fraction'),{range:[0,1],percent:true,history:reports,run,label});plot('kl',series(reports,'post_kl','kl'),{tozero:true,history:reports,run,label});plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run,label});plot('policy',series(reports,'policy_loss'),{history:reports,run,label});plot('critic',series(reports,'critic_loss'),{tozero:true,history:reports,run,label});plot('duration',optimizerSeries(steps,reports,'total_seconds'),{tozero:true,history:reports,run,label});plot('lag',series(reports,'policy_lag_p95'),{tozero:true,history:reports,run,label});plot('gradient',series(reports,'gradient_norm'),{tozero:true,history:reports,run,label});plot('dataset',series(reports,'dataset_rows'),{tozero:true,history:reports,run,label});dynamicPlots(reports,run,label);if(comparison)addComparison(comparison);timeOrigin=run._timeOrigin;document.querySelector('#rows').innerHTML=rowSummary(reports);document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1));bindAxes()}
function extendFloor(history,index=0){const completed=history.filter(report=>Number(report.metrics.episodes)>0||(report.metrics.trajectory_floors?.length??0)>0);if(!completed.length)return;const node=document.querySelector('#floor'),state=node._floor[index],bands=[['min','max'],['p01','p99'],['p05','p95'],['p10','p90'],['p25','p75']],xs=[],ys=[];for(const [band,[low,high]] of bands.entries()){let rows=completed.map(report=>({x:x(report),low:report.metrics.floor_bands?.[low],high:report.metrics.floor_bands?.[high]})).filter(row=>Number.isFinite(row.low)&&Number.isFinite(row.high)),visible=sampled(rows,state.bands[band]);state.bands[band]+=rows.length;xs.push(visible.map(row=>row.x),visible.map(row=>row.x));ys.push(visible.map(row=>row.low),visible.map(row=>row.high))}if(xs.some(values=>values.length))Plotly.extendTraces(node,{x:xs,y:ys},bands.flatMap((_,band)=>[state.start+2*band,state.start+2*band+1]));let points=trajectoryPoints(completed),visible=sampled(points,state.points);state.points+=points.length;if(visible.length)Plotly.extendTraces(node,{x:[visible.map(point=>point.x)],y:[visible.map(point=>point.y)],customdata:[visible.map(point=>[characterNames[point.character],point.updates,point.step])],'marker.color':[visible.map(point=>characterColors[point.character])]},[state.start+10]);const means=completed.map(report=>({x:x(report),y:report.metrics.floor_bands?.mean??report.metrics.floor_mean})).filter(point=>Number.isFinite(point.y)),offset=state.means,line=means.map(point=>({x:point.x,y:state.ema=state.ema===undefined?point.y:Math.max(.01,Math.min(1,Number(ema.value)||.2))*point.y+(1-Math.max(.01,Math.min(1,Number(ema.value)||.2)))*state.ema})),shown=sampled(means,offset),update={x:[shown.map(point=>point.x)],y:[shown.map(point=>point.y)]},traces=[state.start+11];state.means+=means.length;if(state.smooth!==null){visible=sampled(line,offset);update.x.push(visible.map(point=>point.x));update.y.push(visible.map(point=>point.y));traces.push(state.smooth)}if(update.x.some(values=>values.length))Plotly.extendTraces(node,update,traces)}function extendStage(id,reports,key,index){let points=reports.filter(row=>Number.isFinite(Number(row.stage?.[key]))).map(row=>({x:x(row),y:row.stage[key]}));if(!points.length)return;const node=document.querySelector(`#${id}`),state=node._stage[index],visible=sampled(points,state.seen);state.seen+=points.length;if(visible.length)Plotly.extendTraces(node,{x:[visible.map(point=>point.x)],y:[visible.map(point=>point.y)]},[state.trace])}
function appendData(run,reports,steps){run.reports=run.reports.filter(row=>!row._open).concat(reports);const all=monotonic(run.reports);updateSteps(all);const last=all.at(-1),revision=Number(last?.metrics.weights_revision??last?.metrics.updates)||0;run._updateOffset=(last?last._updates:0)-revision;speedMetrics(reports);for(const step of steps)step._updates=run._updateOffset+step.weights_revision;run.optimizer_steps.push(...steps);return all}function appendReports(run,reports,steps,index=0){const all=appendData(run,reports,steps);extendPlot('advantage',series(reports,'advantage_mean','mean_advantage'),index);extendPlot('optimizer-rate',optimizerSeries(steps,[],'optimizer_steps_per_second'),index);extendPlot('row-rate',optimizerSeries(steps,[],'used_rows_per_second'),index);if(selectedMetrics.has('floor_bands'))extendFloor(reports,index);if(selectedMetrics.has('ascension'))extendStage('ascension',reports,'ascension',index);if(selectedMetrics.has('bonus'))extendStage('bonus',reports,'bonus',index);extendPlot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),index);extendPlot('clip',series(reports,'clip_fraction'),index);extendPlot('kl',series(reports,'post_kl','kl'),index);extendPlot('entropy',series(reports,'entropy'),index);extendPlot('policy',series(reports,'policy_loss'),index);extendPlot('critic',series(reports,'critic_loss'),index);extendPlot('duration',optimizerSeries(steps,[],'total_seconds'),index);extendPlot('lag',series(reports,'policy_lag_p95'),index);extendPlot('gradient',series(reports,'gradient_norm'),index);extendPlot('dataset',series(reports,'dataset_rows'),index);for(const [key,id] of Object.entries(metricIds))if(selectedMetrics.has(key))extendPlot(id,series(reports,key),index);if(index===0){const history=all.filter(row=>!row._open);for(const id of ['ascension','bonus'])if(selectedMetrics.has(id))Plotly.relayout(id,{shapes:stageLines(history,run)});document.querySelector('#rows').innerHTML=rowSummary(history);document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1))}}
let refreshing=false;async function refreshLive(){const selected=[versionSelect.value,...compareSelect.value&&compareSelect.value!==versionSelect.value?[compareSelect.value]:[]];if(refreshing||!selected.some(name=>versions[name]?.live))return;refreshing=true;let redraw=false;try{for(const [index,name] of selected.entries()){const run=versions[name];if(!run.live)continue;const url=`${run.live}&reports=${run.report_count}&steps=${run.optimizer_step_count}&promotions=${run.promotion_count}&token=${encodeURIComponent(run.live_token)}`,live=await fetch(url,{cache:'no-store'}).then(response=>{if(!response.ok)throw Error(response.statusText);return response.json()});if(versionSelect.value!==selected[0]||compareSelect.value!==(selected[1]||''))return;if(live.reset){location.reload();return}run.promotions.push(...live.promotions);timeOrigin=run._timeOrigin||0;if(selected.length>1){appendData(run,live.reports,live.optimizer_steps);redraw||=Boolean(live.reports.length||live.optimizer_steps.length||live.promotions.length)}else appendReports(run,live.reports,live.optimizer_steps,index);run.report_count=live.report_count;run.optimizer_step_count=live.optimizer_step_count;run.promotion_count=live.promotion_count;run.live_revision=live.revision;run.status=live.status;run.last_event_time=live.last_event_time}if(redraw)showVersion(false);else showStatus(versions[selected[0]])}catch{document.querySelector('#status').textContent='offline'}finally{refreshing=false}}
lineageSelect.onchange=()=>{showBranches();showVersion()};versionSelect.onchange=()=>showVersion();compareSelect.onchange=()=>showVersion();xaxis.onchange=()=>showVersion();smooth.onchange=()=>showVersion(false);ema.oninput=()=>showVersion(false);subsample.oninput=()=>showVersion(false);showVersion();bindAxes();setTimeout(restoreDashboardState,100);window.addEventListener('beforeunload',saveDashboardState);setInterval(refreshLive,2000)</script>"""
    content = content.replace(
        "['Priority',`1+|Awin|+|Aprogress|+terminal/win bonuses`,`Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.",
        "[manifest.model_version>=66?'Reusable priority':'Priority',manifest.model_version>=66?'initial score, then −3 per use':`1+|Awin|+|Aprogress|+terminal/win bonuses`,manifest.model_version>=66?'Rows remain in the dataset after use, lose three priority points, and retire only below zero. Selection itself is uniform.':'Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.",
    ).replace(
        "['Token-cost bucket',`one bucket per batch`,`Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.",
        "[manifest.model_version>=66?'Eligibility screen':'Token-cost bucket',manifest.model_version>=66?'before optimizer batch':`one bucket per batch`,manifest.model_version>=66?'Forced actions are never admitted. Stale and behavior-ratio-invalid rows are removed before the final batch is built.':'Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.",
    ).replace(
        "['Balanced one-pass batch',`up to ${n(batch)} rows`,`Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset.",
        "[manifest.model_version>=66?'Reusable random batch':'Balanced one-pass batch',`up to ${n(batch)} usable rows`,manifest.model_version>=66?'Rows are sampled uniformly across all compute costs. Used rows remain until their priority falls below zero.':'Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset.",
    )
    target.mkdir(parents=True, exist_ok=True)
    temporary = target / "dashboard.html.tmp"
    temporary.write_text(content)
    temporary.replace(target / "dashboard.html")


class DashboardSource:
    def __init__(self, target):
        self.target = target.resolve()
        self.projectors = {}
        self.lock = threading.Lock()

    def delta(self, name, version, report_count, step_count, promotion_count, token):
        with self.lock:
            run = (self.target / name).resolve()
            if not run.is_relative_to(self.target):
                raise ValueError("invalid run")
            manifest = json.loads((run / "run.json").read_text())
            session = manifest["sessions"][-1]["id"]
            key = manifest["run_id"], session
            projector = self.projectors.get(name)
            if projector is None or projector[0] != key:
                projector = key, MetricsProjector(self.target, run, manifest, session)
                self.projectors[name] = projector
            metrics = projector[1]
            if metrics.cursor > _event_log_size(metrics.path):
                metrics = MetricsProjector(self.target, run, manifest, session)
                self.projectors[name] = key, metrics
            else:
                metrics.cursor = metrics.read(metrics.path, start=metrics.cursor)
            current_report = metrics.render(metrics.current) if metrics.current else None
            reports = metrics.closed + ([current_report] if current_report
                                         and metrics.status != "running" else [])
            reports = [row for row in reports if report_version(row, manifest) == version]
            current = [current_report] if current_report and metrics.status == "running" \
                and report_version(current_report, manifest) == version else []
            steps = [step for row in reports + current
                     for step in row["metrics"].get("optimizer_steps", ())]
            promotions = [row for row in metrics.promotions
                          if not reports or reports[0]["step"] <= row.get("step", 0)
                          <= (current or reports)[-1]["step"]]
            current_token = f"{manifest['run_id']}:{session}:{version}"
            reset = token not in (None, current_token) or report_count > len(reports) \
                or step_count > len(steps) or promotion_count > len(promotions)
            return {
                "token": current_token, "reset": reset, "revision": metrics.cursor,
                "status": metrics.status, "last_event_time": metrics.last_time,
                "reports": reports[report_count:] if not reset else [],
                "optimizer_steps": steps[step_count:] if not reset else [],
                "promotions": promotions[promotion_count:] if not reset else [],
                "report_count": len(reports), "optimizer_step_count": len(steps),
                "promotion_count": len(promotions),
            }


def serve_dashboard(target, host, port):
    target = (target.parent if (target / "run.json").exists() else target).resolve()
    dashboard(target)
    source = DashboardSource(target)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            request = urlparse(self.path)
            try:
                if request.path in ("/", "/dashboard.html"):
                    body, content_type = (target / "dashboard.html").read_bytes(), "text/html"
                elif request.path == "/api":
                    query = parse_qs(request.query)
                    value = source.delta(
                        query["run"][0], int(query["version"][0]),
                        int(query.get("reports", [0])[0]),
                        int(query.get("steps", [0])[0]),
                        int(query.get("promotions", [0])[0]),
                        query.get("token", [None])[0],
                    )
                    if value["reset"]:
                        dashboard(target)
                    body, content_type = json.dumps(value, separators=(",", ":")).encode(), \
                        "application/json"
                else:
                    self.send_error(404); return
            except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
                self.send_error(400, str(error)); return
            self.send_response(200)
            self.send_header("Content-Type", f"{content_type}; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers(); self.wfile.write(body)

        def log_message(self, format, *args):
            pass

    server = http.server.ThreadingHTTPServer((host, port), Handler)
    print(f"Dashboard: http://{host}:{server.server_port}/", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
