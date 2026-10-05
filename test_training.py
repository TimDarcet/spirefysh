import gzip
import io
import json
import logging
import math
import multiprocessing
import subprocess
import sys
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock

import numpy as np
import torch

import model
import train
import rollout
import telemetry
import trajectories
from explanations import explain


def write_events(path, rows):
    with path.open("ab") as output:
        for row in rows:
            output.write((json.dumps({"event_schema": 2} | row) + "\n").encode())


def packed_row(legal=2):
    return (0, (), (), (), np.ones((legal, 1), np.uint32), 0)


class TelemetryTest(unittest.TestCase):
    def test_console_handler_uses_plain_output_without_a_tty(self):
        stream = io.StringIO()
        handler = telemetry._console_handler(stream, False, "INFO")
        logger = logging.Logger("plain", logging.DEBUG); logger.addHandler(handler)
        logger.debug("hidden")
        logger.info("plain")
        output = stream.getvalue()
        self.assertIn("INFO plain", output)
        self.assertNotIn("hidden", output)
        self.assertNotIn("\33", output)
        self.assertTrue(output.isascii())

    def test_console_handler_uses_rich_output_for_a_tty(self):
        stream = io.StringIO()
        handler = telemetry._console_handler(stream, True, "INFO")
        logger = logging.Logger("rich", logging.DEBUG); logger.addHandler(handler)
        logger.warning("▲ warning")
        output = stream.getvalue()
        self.assertIn("\33[", output)
        self.assertIn("▲ warning", output)

    def test_logging_setup_rolls_back_after_failure(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
                telemetry.sts2_sim, "configure_logging", side_effect=RuntimeError("boom")):
            with self.assertRaisesRegex(RuntimeError, "boom"):
                telemetry.configure_logging(temporary, "learner")
        self.assertFalse(telemetry._LOG_ACTIVE)
        self.assertFalse(telemetry._LOG_CAPTURES)
        self.assertFalse(telemetry._LOGGER.handlers)

    def test_logging_filters_and_restores_redirected_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            code = """
from telemetry import configure_logging, emit_event, shutdown_logging
import sys
configure_logging(sys.argv[1], 'learner', 'WARNING')
emit_event({'event': 'heartbeat'})
emit_event({'event': 'checkpoint'})
emit_event({'event': 'slow_update'}, 'WARNING')
print('captured')
shutdown_logging()
print('restored')
"""
            result = subprocess.run(
                [sys.executable, "-c", code, temporary], cwd=Path(__file__).parent,
                capture_output=True, text=True, check=True,
            )
            self.assertNotIn("heartbeat", result.stdout)
            self.assertNotIn("checkpoint", result.stdout)
            self.assertNotIn("\33", result.stdout)
            self.assertIn("WARNING slow_update", result.stdout)
            self.assertIn("captured\nrestored\n", result.stdout)
            events = [json.loads(line) for line in
                      (Path(temporary) / "events.jsonl").read_text().splitlines()]
            self.assertEqual([row["event"] for row in events], [
                "logging_started", "heartbeat", "checkpoint", "slow_update", "log",
            ])

    def test_training_performance_controls(self):
        args = train.parser().parse_args([
            "train", "--disable-post-kl-check", "--mps-empty-cache-updates", "8",
        ])
        self.assertTrue(args.disable_post_kl_check)
        self.assertEqual(args.mps_empty_cache_updates, 8)
        self.assertEqual(args.sampler_threads, 5)
        self.assertEqual(args.dataset_capacity, 131_072)
        self.assertEqual(args.precision, "bf16")
        self.assertEqual(args.uniform_cross_entropy_weight, 0)
        self.assertEqual(train.parser().parse_args([
            "train", "--uniform-cross-entropy-weight", ".2",
        ]).uniform_cross_entropy_weight, .2)
        mixed = train.parser().parse_args([
            "train", "--precision", "bf16_with_fp32_dV",
        ])
        self.assertEqual(mixed.precision, "bf16_with_fp32_dV")
        self.assertEqual(
            [args.boss_1_floor_increment, args.boss_2_floor_increment,
             args.boss_3_floor_increment],
            [10, 11, 10],
        )
        self.assertFalse(vars(args).keys() & {
            "max_policy_lag", "segment_steps", "blended_critic",
            "critic_consistency_weight", "critic_consistency_batch",
            "critic_win_ema_decay", "critic_blend_power", "promotion_trigger_rate",
            "evaluation_max_steps", "evaluation_max_combat_steps",
        })
        self.assertFalse(args.capture_trajectories)
        self.assertFalse(args.terminal_potential)
        self.assertEqual(args.potential_mode, "health_multiplier")
        self.assertTrue(train.parser().parse_args([
            "train", "--terminal-potential", "--potential-mode", "additive",
            "--potential-bias-weight", ".01",
        ]).terminal_potential)
        additive = train.parser().parse_args([
            "train", "--potential-mode", "additive", "--potential-health-weight", ".01",
        ])
        self.assertEqual((additive.potential_mode, additive.potential_health_weight),
                         ("additive", .01))

    def test_uniform_cross_entropy(self):
        probability = torch.tensor([[.6, .3, .1], [.5, .5, .01]])
        log_probability = probability.log()
        legal = torch.tensor([[True, True, True], [True, True, False]])
        expected = -(log_probability[0].mean() + log_probability[1, :2].mean()) / 2
        self.assertAlmostEqual(float(train.uniform_cross_entropy(
            log_probability, legal,
        )), float(expected))
        self.assertAlmostEqual(float(train.uniform_cross_entropy(
            log_probability.flatten(), legal.flatten(),
            torch.tensor([0, 0, 0, 1, 1, 1]), 2,
        )), float(expected))
        self.assertAlmostEqual(float(train.uniform_cross_entropy(
            torch.cat((log_probability[0], log_probability[1, :2])), None,
            torch.tensor([0, 0, 0, 1, 1]), 2,
        )), float(expected))

    def test_fp32_disables_autocast(self):
        class Model:
            def __call__(self, value, *_args, **_kwargs):
                self.autocast = torch.is_autocast_enabled(value.device.type)
                return value, value

        model = Model()
        result = train.predict(model, (torch.ones(1),) * 6, "fp32")
        self.assertFalse(model.autocast)
        self.assertTrue(all(value.dtype == torch.float32 for value in result))

    def test_sampler_seed_streams_are_disjoint(self):
        args = Namespace(
            training_seed=1_000, development_seed=100_000_000,
            promotion_seed=200_000_000, samplers=2, sampler_restarts=3,
        )
        sessions = []
        for session in (1, 2):
            seeds = {
                train.sampler_seed(args, session, worker, generation, ordinal)[0]
                for generation in range(args.sampler_restarts + 1)
                for worker in range(args.samplers)
                for ordinal in range(100)
            }
            self.assertEqual(len(seeds), 800)
            sessions.append(seeds)
        self.assertTrue(sessions[0].isdisjoint(sessions[1]))
        self.assertEqual(train.sampler_seed(args, 1, 0, 0), (10_001_000, 8, 20_001_000))

    def test_sampler_seed_stream_stops_at_namespace_boundary(self):
        args = Namespace(
            training_seed=1_000, development_seed=100_000_000,
            promotion_seed=200_000_000, samplers=2, sampler_restarts=3,
        )
        stream = args.sampler_restarts * args.samplers + args.samplers - 1
        last = (train.SAMPLER_SEED_BLOCK - 1 - stream) // 8
        train.sampler_seed(args, 1, 1, 3, last)
        with self.assertRaisesRegex(RuntimeError, "namespace exhausted"):
            train.sampler_seed(args, 1, 1, 3, last + 1)

    def test_native_batch_honors_seed_stride(self):
        env = train.sts2_sim.Batch(2, 100, None, ascension=0, seed_stride=4)
        self.assertEqual(env.seeds(), [100, 104])
        env.reset([0])
        self.assertEqual(env.seeds(), [108, 104])

    def test_sampler_session_claim_survives_checkpoint_rollback(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "run.json"
            manifest = {"sampler_session": 2}
            self.assertEqual(train.claim_sampler_session(path, manifest, 2), 3)
            reloaded = json.loads(path.read_text())
            self.assertEqual(train.claim_sampler_session(path, reloaded, 2), 4)

    def test_sampler_session_highwater_includes_failed_legacy_attempts(self):
        source = {"sampler_session": 7, "session_id": 2}
        manifest = {"sampler_session": 1, "sessions": [
            {"id": 1}, {"id": 2}, {"id": 3}, {"id": 4},
        ]}
        self.assertEqual(train.sampler_session_highwater(source, manifest, 4), 8)
        self.assertEqual(train.sampler_session_highwater(
            {"sampler_session": 0, "session_id": 1},
            {"sessions": [{"id": 1}, {"id": 2}]}, 2,
        ), 1)
        self.assertEqual(train.sampler_session_highwater(
            source, {"sessions": [{"id": value} for value in range(1, 6)]}, 5,
        ), 8)

    def test_sampler_diagnostic_captures_the_stalled_batch(self):
        context = multiprocessing.get_context("spawn")
        diagnostics = (
            context.RawArray("i", 2), context.RawArray("d", 2),
            context.RawArray("q", 2), context.RawArray(
                "q", [-1] * (4 * len(train.SAMPLER_STATE_FIELDS)),
            ),
            context.RawArray("q", [-1] * (4 * 3)),
        )
        collector = train.RolloutCollector.__new__(train.RolloutCollector)
        collector.args = Namespace(samplers=2, envs=2)
        collector.worker = 1; collector.iteration = 7; collector.diagnostics = diagnostics
        collector.episode_steps = np.array([8, 9])
        collector.combat_steps = np.array([3, 4])
        collector.env = mock.Mock()
        collector.env.seeds.return_value = [101, 102]
        collector.env.characters.return_value = [3, 4]
        collector.env.stats.return_value = [
            (1, 2, 30, 40, 5, 6, 20, 10, .5, 7),
            (2, 3, 31, 41, 6, 7, 21, 11, .6, 8),
        ]
        collector.env.diagnostic_stats.return_value = [[0] * 14, list(range(14))]
        collector.mark_phase("policy", True)
        np.frombuffer(diagnostics[4], np.int64).reshape(2, 2, 3)[1, 0] = [5, 2, 7]
        diagnostic = train.sampler_diagnostic(diagnostics, 1, 2)
        self.assertEqual((diagnostic["phase"], diagnostic["iteration"]), ("policy", 7))
        self.assertEqual(diagnostic["states"][0], {
            "seed": 101, "character": 3, "act": 1, "floor": 2, "hp": 30,
            "max_hp": 40, "phase": 5, "turn": 6, "player_hp": 20,
            "enemy_hp": 10, "canonical_progress": 7, "episode_steps": 8,
            "combat_steps": 3, "deck_cards": 0, "relics": 0, "potion_slots": 0,
            "run_queue": 0, "hand": 0, "draw": 0, "discard": 0, "exhaust": 0,
            "combat_queue": 0, "auto_plays": 0, "enemies": 0, "player_powers": 0,
            "enemy_powers": 0, "enemy_move_history": 0, "policy_stage": "environment",
            "policy_choice": 2, "policy_action_kind": 7,
        })
        collector.diagnostics = (
            context.RawArray("i", 2), context.RawArray("d", 2), context.RawArray("q", 2),
            context.RawArray("q", [-1] * (4 * 11)),
        )
        collector.mark_phase("policy", True)
        legacy = train.sampler_diagnostic(collector.diagnostics, 1, 2)["states"][0]
        self.assertEqual(legacy["canonical_progress"], 7)
        self.assertIsNone(legacy["policy_stage"])

    def test_native_sampler_diagnostics_have_fixed_width(self):
        rows = train.sts2_sim.Batch(2, 7, 0).diagnostic_stats()
        self.assertEqual(len(rows), 2)
        self.assertTrue(all(len(row) == 14 for row in rows))
        self.assertEqual(rows[0][:4], [11, 1, 2, 0])

    def test_native_policy_reports_its_completed_stage(self):
        env = train.sts2_sim.Batch(2, 7, 0)
        env.load_policy(train.export_value_model(
            None, train.Agent(dict(env.token_layout())).eval(), env.fingerprint(), 1, 0, True,
        ))
        diagnostics = np.empty((2, 3), np.int64)
        env.policy(advance=True, diagnostics=diagnostics)
        self.assertTrue((diagnostics[:, 0] == 6).all())
        self.assertTrue((diagnostics[:, 1:] >= 0).all())

    def test_trajectory_capture_flag(self):
        self.assertTrue(train.parser().parse_args([
            "train", "--capture-trajectories",
        ]).capture_trajectories)

    def test_learning_rate_warmup_uses_global_weights_revision(self):
        optimizer = Namespace(param_groups=[
            {"lr_scale": 1.}, {"lr_scale": 2.}, {"lr_scale": .5},
        ])
        train.set_learning_rate(optimizer, .01, 200, 0)
        np.testing.assert_allclose(
            [group["lr"] for group in optimizer.param_groups], [.00005, .0001, .000025]
        )
        train.set_learning_rate(optimizer, .01, 200, 199)
        np.testing.assert_allclose(
            [group["lr"] for group in optimizer.param_groups], [.01, .02, .005]
        )
        train.set_learning_rate(optimizer, .01, 200, 2000)
        np.testing.assert_allclose(
            [group["lr"] for group in optimizer.param_groups], [.01, .02, .005]
        )

    def test_checkpoint_optimizer_configuration(self):
        checkpoint = {"optimizer": {"param_groups": [{
            "params": [], "betas": (.9, .99), "eps": 1e-5,
        }]}}
        self.assertEqual(train.checkpoint_optimizer(checkpoint), train.OPTIMIZER_CONFIG)
        train.require_optimizer_config(checkpoint)
        checkpoint["optimizer"]["param_groups"][0]["betas"] = (.9, .999)
        checkpoint.pop("optimizer_config")
        with self.assertRaisesRegex(ValueError, "does not match configured optimizer"):
            train.require_optimizer_config(checkpoint)

    def test_model_schema_comes_from_simulator(self):
        schema = train.sts2_sim.model_schema()
        self.assertEqual((train.FEATURE_VERSION, train.MODEL_VERSION), schema[:2])
        self.assertEqual(train.TOKEN_SPECS, tuple(tuple(row) for row in schema[5]))

    def manifest(self, run_id, session, parent=None):
        return {
            "schema": 2, "run_id": run_id, "lineage_id": "lineage",
            "model_version": train.MODEL_VERSION,
            "telemetry_window_decisions": 100,
            "stages": [{"ascension": 0, "bonus": 24}],
            "sessions": [{"id": session, "log": f"events/{session:06}.jsonl",
                          "parent": parent}],
        }

    def test_incremental_cursor_ignores_partial_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            log = run / "events/000001.jsonl"
            write_events(log, [
                {"event": "checkpoint", "time": 1, "step": 0, "stage": 0,
                 "weights_revision": 0, "policy_revision": 0,
                 "training_elapsed_seconds": 0},
                {"event": "sample_packet", "time": 2, "step": 40, "stage": 0,
                 "policy_revision": 4, "resolved_decisions_total": 40,
                 "packet_sampled_decisions": 80, "admitted_rows": 30,
                 "discarded_decisions": 5, "forced_rows": 3,
                 "budget_excess_rows": 2, "capacity_dropped_rows": 4,
                 "collect_seconds": 2, "queue_delay_seconds": .5,
                 "cache": {"card_hits": 9, "card_misses": 1},
                 "mcts": {"roots": 2, "simulations": 10, "seconds": 2},
                 "episodes": [{"character": 2, "floor": 12, "won": True,
                               "step_cap": False, "combat_cap": False,
                               "empty_actions": False, "length": 20,
                               "completion_seconds": 1, "iteration": 5,
                               "policy_revision_min": 2,
                               "policy_revision_max": 4},
                              {"character": 0, "floor": 1, "won": False,
                               "recovery": True}]},
                {"event": "training_batch", "time": 3, "step": 40, "stage": 0,
                 "weights_revision": 1, "policy_revision": 1,
                 "attempted_rows": 20, "fresh_rows": 20,
                 "policy_trained_rows": 20, "critic_trained_rows": 20,
                 "policy_outcome": "accepted", "commit_kind": "full",
                 "accepted_trajectories": {
                     "count": 2, "sampled_rows": 20,
                     "floor_counts": {"12": 1, "20": 1},
                     "length_mean": 30, "length_max": 40,
                     "actionable_length_mean": 20, "actionable_length_max": 25,
                     "policy_span_mean": 2, "policy_span_max": 3,
                     "policy_age_mean": 3, "policy_age_max": 4,
                     "character_counts": [1, 0, 1, 0, 0],
                     "log_ratio": {
                         "count": 20, "mean": .1, "min": -.2, "max": .3,
                         "abs_mean": .2, "abs_max": .3,
                         "abs_p50": .2, "abs_p90": .3, "abs_p99": .3,
                     },
                 },
                 "retired_rows": 4, "policy_lag_counts": {"0": 10, "1": 10},
                 "advantage_mean": 2, "advantage_stddev": 3,
                 "policy_loss": 2, "uniform_cross_entropy": 1.5, "critic_loss": 3,
                 "search_consistency_loss": .25, "gradient_clipped": True,
                 "critic_explained_reward_variance": .5, "total_seconds": 1,
                 "pause_seconds": .25,
                 "learning_rates": [1e-4, 2e-4, 5e-5],
                 "critic_explained_reward_variance_by_floor": {
                     "12": {"value": .5, "target_variance": .25, "rows": 20}},
                 "training_elapsed_seconds": 3},
                {"event": "dataset_pruned", "time": 3.5, "step": 40, "stage": 0,
                 "incomplete_rows": 2},
            ])
            projector = train.MetricsProjector(root, run, manifest, 1)
            cursor = projector.cursor
            event = json.dumps({"event_schema": 2, "event": "heartbeat", "time": 4, "step": 41,
                                "stage": 0, "dataset_rows": 7}).encode()
            with log.open("ab") as output:
                output.write(event[:len(event) // 2])
            self.assertEqual(projector.read(log, start=cursor), cursor)
            with log.open("ab") as output:
                output.write(event[len(event) // 2:] + b"\n")
            projector.cursor = projector.read(log, start=cursor)
            metrics = projector.value()["reports"][-1]["metrics"]
            self.assertEqual(projector.cursor, log.stat().st_size)
            self.assertEqual(metrics["floor_bands"]["median"], 12)
            self.assertEqual(metrics["characters"][2]["wins"], 1)
            self.assertEqual(metrics["win_rate"], 1)
            self.assertEqual(metrics["recovery_episodes"], 1)
            self.assertEqual(metrics["boss_entries"], 1)
            self.assertEqual(metrics["boss_entry_rate"], 1)
            self.assertEqual(metrics["boss_conversion"], 1)
            self.assertEqual(len(metrics["win_rate_interval"]), 2)
            self.assertEqual(metrics["policy_loss"], 2)
            self.assertEqual(metrics["uniform_cross_entropy"], 1.5)
            self.assertEqual(metrics["critic_explained_reward_variance"], .5)
            self.assertEqual(metrics["critic_explained_reward_variance_by_floor"], {
                "12": {"value": .5, "target_variance": .25, "rows": 20},
            })
            self.assertEqual(metrics["critic_floor_conditioned_explained_reward_variance"], .5)
            self.assertEqual(metrics["search_consistency_loss"], .25)
            self.assertEqual(metrics["backbone_learning_rate"], 1e-4)
            self.assertEqual(metrics["policy_learning_rate"], 2e-4)
            self.assertEqual(metrics["critic_learning_rate"], 5e-5)
            self.assertEqual(metrics["gradient_clipped"], 1)
            self.assertEqual(metrics["pause_seconds"], .25)
            self.assertEqual(metrics["advantage_stddev"], 3)
            self.assertEqual(metrics["policy_lag_mean"], .5)
            self.assertEqual(metrics["policy_lag_p95"], 1)
            self.assertEqual(metrics["policy_lag_max"], 1)
            self.assertEqual(metrics["trajectory_policy_span_max"], 2)
            self.assertEqual(metrics["trajectory_arrival_lag_max"], 2)
            self.assertEqual(metrics["card_cache_hit_rate"], .9)
            self.assertEqual(metrics["mcts_simulations_per_root"], 5)
            self.assertEqual(metrics["dataset_rows"], 7)
            self.assertEqual(metrics["optimizer_steps_per_second"], 1)
            self.assertEqual(metrics["used_rows_per_second"], 20)
            self.assertEqual(metrics["decisions_trained"], 20)
            self.assertEqual(metrics["accepted_trajectories"], 2)
            self.assertEqual(metrics["accepted_trajectory_sampled_rows"], 20)
            self.assertEqual(metrics["accepted_trajectory_floor_mean"], 16)
            self.assertEqual(metrics["accepted_trajectory_floor_median"], 16)
            self.assertEqual(metrics["accepted_trajectory_floor_p90"], 19.2)
            self.assertEqual(metrics["accepted_trajectory_length_mean"], 30)
            self.assertEqual(metrics["accepted_trajectory_length_max"], 40)
            self.assertEqual(metrics["accepted_trajectory_policy_age_mean"], 3)
            self.assertEqual(metrics["accepted_trajectory_character_counts"], [1, 0, 1, 0, 0])
            self.assertEqual(metrics["accepted_log_ratio_rows"], 20)
            self.assertEqual(metrics["accepted_log_ratio_mean"], .1)
            self.assertEqual(metrics["accepted_abs_log_ratio_mean"], .2)
            self.assertEqual(metrics["accepted_abs_log_ratio_max"], .3)
            self.assertEqual(metrics["optimizer_steps"], [{
                "step": 40, "weights_revision": 1, "seconds": 3,
                "decisions_trained": 20,
                "optimizer_steps_per_second": 1,
                "used_rows_per_second": 20, "total_seconds": 1,
                "backbone_learning_rate": 1e-4,
                "policy_learning_rate": 2e-4,
                "critic_learning_rate": 5e-5,
            }])
            self.assertEqual(metrics["dataset_rollout_dropped"], 3)
            self.assertEqual(metrics["dataset_budget_dropped"], 2)
            self.assertEqual(metrics["dataset_forced_dropped"], 3)
            self.assertEqual(metrics["dataset_capacity_dropped"], 4)
            self.assertEqual(metrics["dataset_incomplete_dropped"], 2)
            self.assertEqual(metrics["dataset_retired"], 4)
            for key in ("step", "stage", "resolved_decisions_total", "policy_revision",
                        "update_attempt"):
                self.assertNotIn(key, metrics)

    def test_projection_cache_is_reused_until_the_log_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            log = run / "events/000001.jsonl"
            write_events(log, [{"event": "checkpoint", "time": 1, "step": 0, "stage": 0}])
            expected = telemetry._cached_projection(root, run, manifest, 1)
            with mock.patch.object(telemetry, "MetricsProjector",
                                   side_effect=AssertionError("cache miss")):
                self.assertEqual(telemetry._cached_projection(root, run, manifest, 1), expected)
                write_events(log, [{"event": "checkpoint", "time": 2,
                                    "step": 1, "stage": 0}])
                with self.assertRaisesRegex(AssertionError, "cache miss"):
                    telemetry._cached_projection(root, run, manifest, 1)

    def test_closed_event_logs_are_compressed_and_remain_readable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            log = run / "events/000001.jsonl"
            write_events(log, [
                {"event": "session_start", "time": 1, "step": 0, "stage": 0,
                 "role": "learner"},
                {"event": "checkpoint", "time": 2, "step": 3, "stage": 0},
            ])
            for name in ("initial.json", "latest.json"):
                (run / name).write_text(json.dumps({"event_log": "events/000001.jsonl"}))
            with mock.patch.object(telemetry, "_LOG_COMPRESSION_BYTES", 1):
                telemetry._compress_event_log(run, log)
            compressed = log.with_suffix(".jsonl.gz")
            self.assertFalse(log.exists())
            self.assertEqual(telemetry.event_logs(run), [compressed])
            self.assertEqual(json.loads((run / "run.json").read_text())
                             ["sessions"][0]["log"], "events/000001.jsonl.gz")
            self.assertEqual(json.loads((run / "latest.json").read_text())
                             ["event_log"], "events/000001.jsonl.gz")
            manifest = json.loads((run / "run.json").read_text())
            self.assertEqual(telemetry.MetricsProjector(root, run, manifest, 1)
                             .value()["reports"][-1]["step"], 3)

    def test_projector_tracks_policy_rejections(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            write_events(run / "events/000001.jsonl", [
                {"event": "training_batch", "time": 1, "step": 40, "stage": 0,
                 "policy_outcome": "pre_kl_rejected", "commit_kind": "critic_only",
                 "fresh_rows": 5, "critic_trained_rows": 5,
                 "rejected_trajectories": {
                     "count": 1, "sampled_rows": 5, "floor_counts": {"10": 1},
                     "length_mean": 20, "length_max": 20,
                     "actionable_length_mean": 15, "actionable_length_max": 15,
                     "policy_span_mean": 2, "policy_span_max": 2,
                     "policy_age_mean": 3, "policy_age_max": 3,
                     "character_counts": [1, 0, 0, 0, 0],
                     "log_ratio": {
                         "count": 5, "mean": .1, "min": -.2, "max": .3,
                         "abs_mean": .2, "abs_max": .3,
                     },
                     "trigger_log_ratio": {
                         "count": 1, "mean": .6, "min": .6, "max": .6,
                         "abs_mean": .6, "abs_max": .6,
                     },
                 },
                 "critic_explained_reward_variance": .2,
                 "critic_explained_reward_variance_by_floor": {
                     "10": {"value": .2, "target_variance": .25, "rows": 5}},
                 "total_seconds": 2},
                {"event": "training_batch", "time": 2, "step": 40, "stage": 0,
                 "policy_outcome": "post_kl_rejected", "commit_kind": "critic_only",
                 "fresh_rows": 7, "critic_trained_rows": 7,
                 "rejected_trajectories": {
                     "count": 1, "sampled_rows": 7, "floor_counts": {"20": 1},
                     "length_mean": 40, "length_max": 40,
                     "actionable_length_mean": 25, "actionable_length_max": 25,
                     "policy_span_mean": 4, "policy_span_max": 4,
                     "policy_age_mean": 5, "policy_age_max": 5,
                     "character_counts": [0, 0, 1, 0, 0],
                     "log_ratio": {
                         "count": 7, "mean": -.1, "min": -.5, "max": .4,
                         "abs_mean": .4, "abs_max": .5,
                     },
                     "trigger_log_ratio": {
                         "count": 2, "mean": -.55, "min": -.6, "max": -.5,
                         "abs_mean": .55, "abs_max": .6,
                     },
                 },
                 "critic_explained_reward_variance": .8,
                 "critic_explained_reward_variance_by_floor": {
                     "10": {"value": .8, "target_variance": .5, "rows": 7}},
                 "total_seconds": 2},
            ])
            metrics = train.MetricsProjector(root, run, manifest, 1).value()["reports"][-1]["metrics"]
            self.assertEqual(metrics["optimizer_steps_per_second"], .5)
            self.assertEqual(metrics["used_rows_per_second"], 3)
            self.assertEqual(metrics["dataset_kl_dropped"], 5)
            self.assertEqual(metrics["dataset_post_kl_dropped"], 7)
            self.assertAlmostEqual(metrics["critic_explained_reward_variance"], .55)
            floor = metrics["critic_explained_reward_variance_by_floor"]["10"]
            self.assertAlmostEqual(floor["value"], 3.05 / 4.75)
            self.assertAlmostEqual(floor["target_variance"], 4.75 / 12)
            self.assertEqual(floor["rows"], 12)
            self.assertAlmostEqual(
                metrics["critic_floor_conditioned_explained_reward_variance"], 3.05 / 4.75
            )
            self.assertEqual(metrics["rejected_trajectories"], 2)
            self.assertEqual(metrics["rejected_trajectory_sampled_rows"], 12)
            self.assertEqual(metrics["rejected_trajectory_floor_mean"], 15)
            self.assertEqual(metrics["rejected_trajectory_length_mean"], 30)
            self.assertEqual(metrics["rejected_trajectory_length_max"], 40)
            self.assertEqual(metrics["rejected_trajectory_policy_span_mean"], 3)
            self.assertEqual(metrics["rejected_trajectory_policy_age_mean"], 4)
            self.assertEqual(metrics["rejected_trajectory_character_counts"], [1, 0, 1, 0, 0])
            self.assertEqual(metrics["rejected_log_ratio_rows"], 12)
            self.assertAlmostEqual(metrics["rejected_log_ratio_mean"], -1 / 60)
            self.assertAlmostEqual(metrics["rejected_abs_log_ratio_mean"], 19 / 60)
            self.assertEqual(metrics["rejected_log_ratio_min"], -.5)
            self.assertEqual(metrics["rejected_log_ratio_max"], .4)
            self.assertEqual(metrics["rejected_abs_log_ratio_max"], .5)
            self.assertEqual(metrics["rejected_trigger_log_ratio_rows"], 3)
            self.assertAlmostEqual(metrics["rejected_trigger_log_ratio_mean"], -1 / 6)
            self.assertAlmostEqual(metrics["rejected_trigger_abs_log_ratio_mean"], 17 / 30)
            self.assertEqual(metrics["rejected_trigger_abs_log_ratio_max"], .6)

    def test_projector_combines_distributions_instead_of_batch_statistics(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            write_events(run / "events/000001.jsonl", [
                {"event": "training_batch", "time": 1, "step": 1, "stage": 0,
                 "fresh_rows": 2, "advantage_mean": 1, "advantage_stddev": 0,
                 "policy_lag_counts": {"0": 1, "1": 1}},
                {"event": "training_batch", "time": 2, "step": 2, "stage": 0,
                 "fresh_rows": 2, "advantage_mean": 3, "advantage_stddev": 0,
                 "policy_lag_counts": {"1": 1, "2": 1}},
            ])
            metrics = train.MetricsProjector(root, run, manifest, 1).value()["reports"][-1]["metrics"]
            self.assertEqual(metrics["advantage_mean"], 2)
            self.assertEqual(metrics["advantage_stddev"], 1)
            self.assertEqual(metrics["policy_lag_mean"], 1)
            self.assertAlmostEqual(metrics["policy_lag_p95"], 1.85)
            self.assertEqual(metrics["policy_lag_max"], 2)

    def test_critic_explained_reward_variance(self):
        targets = torch.tensor([0., 1.])
        midpoint = torch.tensor([.5, .5])
        self.assertEqual(float(train.critic_explained_reward_variance(targets, targets)), 1)
        self.assertEqual(float(train.critic_explained_reward_variance(midpoint, targets)), 0)
        self.assertEqual(float(train.critic_explained_reward_variance(targets, targets[:1].repeat(2))), 0)
        _, conditioned, floors = train.critic_explained_reward_variance(
            torch.cat((targets, midpoint)), torch.cat((targets, targets)), [1, 1, 2, 2]
        )
        self.assertEqual(conditioned, .5)
        self.assertEqual(floors, {
            "1": {"value": 1., "target_variance": .25, "rows": 2},
            "2": {"value": 0., "target_variance": .25, "rows": 2},
        })

    def test_branch_reads_parent_only_to_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); parent = root / "parent"; child = root / "child"
            for run in (parent, child):
                (run / "events").mkdir(parents=True)
            parent_manifest = self.manifest("parent", 1)
            parent_log = parent / "events/000001.jsonl"
            write_events(parent_log, [
                {"event": "session_start", "role": "learner", "time": 1},
                {"event": "sample_packet", "time": 2, "step": 40, "stage": 0,
                 "packet_sampled_decisions": 10, "episodes": []},
                {"event": "checkpoint", "time": 3, "step": 40, "stage": 0},
            ])
            cutoff = parent_log.stat().st_size
            write_events(parent_log, [{"event": "sample_packet", "time": 4, "step": 80,
                                       "stage": 0, "packet_sampled_decisions": 100,
                                       "episodes": []}])
            (parent / "run.json").write_text(json.dumps(parent_manifest))
            parent_ref = {"run_id": "parent", "session_id": 1, "event_end": cutoff}
            child_manifest = self.manifest("child", 1, parent_ref)
            child_log = child / "events/000001.jsonl"
            write_events(child_log, [
                {"event": "session_start", "role": "learner", "time": 5,
                 "parent_checkpoint_step": 40},
                {"event": "sample_packet", "time": 6, "step": 70, "stage": 0,
                 "packet_sampled_decisions": 20, "episodes": []},
            ])
            (child / "run.json").write_text(json.dumps(child_manifest))
            reports = train.MetricsProjector(root, child, child_manifest, 1).value()["reports"]
            self.assertEqual(sum(row["metrics"]["sampled_decisions"] for row in reports), 30)

    def test_checkpoint_hash_fallback_must_be_unambiguous(self):
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary) / "run"; (run / "events").mkdir(parents=True)
            checkpoint = run / "checkpoint.pt"; checkpoint.write_bytes(b"checkpoint")
            digest = train.sha256_file(checkpoint)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            log = run / "events/000001.jsonl"
            event = {"event": "checkpoint", "session_id": 1, "step": 12,
                     "weights_revision": 3, "sha256": digest}
            write_events(log, [event])
            self.assertEqual(train.checkpoint_origin(checkpoint, digest)["event_end"],
                             log.stat().st_size)
            write_events(log, [event])
            self.assertIsNone(train.checkpoint_origin(checkpoint, digest))

    def test_dashboard_rebuilds_metrics_from_events(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1); manifest["model_version"] = 999
            (run / "run.json").write_text(json.dumps(manifest))
            write_events(run / "events/000001.jsonl", [{
                "event": "checkpoint", "time": 1, "step": 0, "stage": 0,
                "weights_revision": 0, "policy_revision": 0,
                "training_elapsed_seconds": 0,
            }])
            nested = root / "group/nested"; nested.mkdir(parents=True)
            nested_manifest = self.manifest("nested", 1)
            (nested / "run.json").write_text(json.dumps(nested_manifest))
            train.dashboard(root)
            html = (root / "dashboard.html").read_text()
            self.assertNotIn('"step":0', html)
            self.assertIn('"group/nested"', html)
            self.assertTrue(all(path in html for path in ("/api/catalog", "/api/run?run=")))
            self.assertEqual(
                train.DashboardSource(root).load("run", 999)["reports"][0]["step"], 0,
            )
            self.assertFalse((run / "live.js").exists())
            self.assertNotIn("<label>Branch", html)
            self.assertTrue(all(label in html for label in (
                "Experiment", "Compare", "Metrics", "Subsample", "Optimizer updates",
                "PPO policy loss", "Pre-clip global gradient norm",
                "Decisions trained on",
                "Committed optimizer updates/s", "Critic rows processed/s",
                "Optimizer update time (seconds)",
                "Training-row policy lag, p95 (revisions)",
                "Data pipeline counters",
                "Outcomes", "Policy learning", "Critic learning", "Performance",
                "Backbone learning rate", "Policy-head learning rate",
                "Critic learning rate",
            )))
            self.assertIn("weights_revision??report.metrics.updates", html)
            self.assertIn("xaxis.value==='decisions'?trained(report)", html)
            self.assertIn("step.decisions_trained??step.step", html)
            self.assertIn("advantage_mean:['mean_advantage']", html)
            self.assertIn("optimizerSeries(steps,reports,'used_rows_per_second')", html)
            self.assertIn("function addComparison", html)
            self.assertIn("function clipComparison", html)
            self.assertIn("setFullRange([primary])", html)
            self.assertIn("compareSelect.onchange=()=>showVersion()", html)
            self.assertIn(".localeCompare(", html)
            self.assertIn("latest=recency.at(-1)", html)
            self.assertIn("names=[...recency].sort(", html)
            self.assertIn("versionSelect.value===latest", html)
            self.assertNotIn("V${versions[name].version}", html)
            self.assertNotIn("Spirefysh V${run.version}", html)
            self.assertIn("id=legend class=legend", html)
            self.assertIn("plotly_relayout", html)
            self.assertIn("showlegend:false", html)
            self.assertIn("95% confidence", html)
            self.assertNotIn("error_y", html)
            self.assertIn("fillcolor:withAlpha(color,.12)", html)
            self.assertIn("uirevision:uiRevision", html)
            self.assertIn("id=subsample type=number min=1 step=1 value=3", html)
            self.assertIn("appendReports(run,live.reports,live.optimizer_steps,index)", html)
            self.assertIn("critic_explained_reward_variance", html)
            self.assertIn("critic_floor_conditioned_explained_reward_variance", html)
            self.assertIn("availableMetricKeys()", html)
            self.assertIn("metricPicker.onchange", html)
            self.assertIn("</div><details class=panel><summary>Metrics", html)
            self.assertNotIn("lines+markers", html)
            self.assertIn("Plotly.extendTraces", html)
            self.assertIn("fetch(url", html)
            self.assertIn("row.step>step", html)

    def test_api_json_replaces_non_finite_metrics_with_null(self):
        value = {"gradient_norm": math.nan, "steps": [math.inf, -math.inf, 1.]}
        self.assertEqual(
            json.loads(train.json_bytes(value)),
            {"gradient_norm": None, "steps": [None, None, 1.]},
        )

    def test_dashboard_source_returns_only_new_points(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            log = run / "events/000001.jsonl"
            write_events(log, [{
                "event": "training_batch", "time": 1, "step": 40, "stage": 0,
                "weights_revision": 1, "policy_revision": 1,
                "policy_outcome": "accepted", "commit_kind": "full",
                "critic_trained_rows": 20, "total_seconds": 2,
                "training_elapsed_seconds": 2,
            }])
            source = train.DashboardSource(root)
            first = source.delta("run", train.MODEL_VERSION, 0, 0, 0, None)
            self.assertEqual(first["reports"], [])
            self.assertEqual(first["optimizer_steps"][0]["used_rows_per_second"], 10)
            write_events(log, [{"event": "heartbeat", "time": 3, "step": 120,
                                "stage": 0, "training_elapsed_seconds": 3}])
            second = source.delta("run", train.MODEL_VERSION, 0, 1, 0, first["token"])
            self.assertEqual(len(second["reports"]), 1)
            self.assertEqual(second["optimizer_steps"], [])

    def test_legacy_heartbeat_does_not_create_a_zero_step(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            write_events(run / "events/000001.jsonl", [
                {"event_schema": 1, "event": "report", "time": 1, "step": 120,
                 "metrics": {"seconds": 10, "decisions_per_second": 12}},
                {"event_schema": 1, "event": "heartbeat", "time": 2,
                 "decisions": 130, "updates": 2},
                {"event": "session_start", "role": "learner", "time": 3,
                 "step": 140, "stage": 0, "training_elapsed_seconds": 12},
            ])
            reports = train.MetricsProjector(root, run, manifest, 1).value()["reports"]
            self.assertEqual([row["step"] for row in reports], [120, 140])

    def test_potential_gae(self):
        def values(canonical, expected, gamma=1, gae_lambda=1, won=False,
                   potentials=None, terminal_potential=0, field="advantage", increments=None):
            length = len(canonical)
            trajectory = {
                "rows": [packed_row() for _ in range(length)], "choices": [0] * length,
                "old_log": [0] * length,
                "critic_values": expected,
                "canonical_progress": canonical, "phases": [0] * length,
                "win_rewards": [0] * (length - 1) + [won],
                "terminals": [False] * (length - 1) + [True],
                "characters": [0] * length, "versions": [0] * length,
                "potentials": np.zeros(length) if potentials is None else potentials,
                "terminal_potential": terminal_potential,
            }
            dataset = train.ExperienceDataset()
            parameters = {} if increments is None else dict(zip(
                ("boss_1_floor_increment", "boss_2_floor_increment",
                 "boss_3_floor_increment"), increments,
            ))
            dataset.add({"trajectories": [trajectory]}, Namespace(
                critic_only=False, gae_gamma=gamma, gae_lambda=gae_lambda, **parameters,
            ))
            return dataset.data[field][:length]

        maximum = train.CATEGORIES - 1
        expected = np.array([.2, .4, .6])
        np.testing.assert_allclose(
            values([0, 1, 2], expected), 2 / maximum - expected, atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 1, 2], np.arange(3) / maximum, .5, .5),
            np.array([.5, 0, 0]) / maximum, atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 1, 2], expected, .5, .5, field="critic_target"),
            np.array([.5, 1, 2]) / maximum, atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 72], np.array([0, 72]) / maximum, won=True),
            np.array([1, 10 / maximum]), atol=1e-7,
        )
        potentials = np.array([.1, .2])
        np.testing.assert_allclose(
            values([0, 72], 1 - potentials, won=True, potentials=potentials),
            0, atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 72], [0, 0], won=True, potentials=potentials,
                   field="critic_target"),
            1 - potentials, atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 0], [0, 0], gamma=.5, potentials=[.1, .2]),
            [-.1, -.2], atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 0], [0, 0], gamma=.5, potentials=[.1, .2],
                   field="critic_target"),
            [-.1, -.2], atol=1e-7,
        )
        np.testing.assert_allclose(
            values([0, 0], [0, 0], gamma=.5, potentials=[.1, .2],
                   terminal_potential=.4, field="critic_target"),
            [0, 0], atol=1e-7,
        )
        np.testing.assert_allclose(
            values([25], [0], increments=(20, 21, 30)), [35 / maximum], atol=1e-7,
        )
        np.testing.assert_allclose(
            values([54], [0], increments=(20, 21, 30)), [74 / maximum], atol=1e-7,
        )
        np.testing.assert_allclose(
            values([72], [0], won=True, increments=(20, 21, 30)),
            [122 / maximum], atol=1e-7,
        )

    def test_forced_actions_do_not_advance_gae_clock(self):
        rows = [packed_row(legal) for legal in (1, 2, 1, 2, 1)]
        trajectory = {
            "rows": rows, "choices": [0] * 5, "old_log": [0] * 5,
            "critic_values": [0] * 5, "canonical_progress": [0] * 5,
            "phases": [0] * 5, "win_rewards": [0, 0, 0, 0, 1],
            "terminals": [False, False, False, False, True], "characters": [0] * 5,
            "versions": [10, 11, 12, 13, 14], "potentials": [0] * 5,
        }
        dataset = train.ExperienceDataset()
        counts = dataset.add({"trajectories": [trajectory]}, Namespace(
            critic_only=True, gae_gamma=.5, gae_lambda=.5,
        ))

        self.assertEqual(counts, (5, 3, 0))
        self.assertEqual(dataset.rows, [rows[1], rows[3]])
        np.testing.assert_allclose(dataset.data["advantage"][:2], [.25, 1])
        np.testing.assert_allclose(dataset.data["critic_target"][:2], [.5, 1])
        np.testing.assert_array_equal(dataset.data["version"][:2], [11, 13])
        np.testing.assert_allclose(dataset.data["priority"][:2], [5.25, 10])

        limited = train.ExperienceDataset()
        self.assertEqual(limited.add({"trajectories": [trajectory]}, Namespace(
            critic_only=False, gae_gamma=.5, gae_lambda=.5,
        ), limit=3), (3, 2, 2))
        self.assertEqual(limited.rows, [rows[1]])
        np.testing.assert_array_equal(limited.data["id"][:1], [1])

        compressed = {
            key: [trajectory[key][index] for index in (1, 3)]
            for key in (
                "rows", "choices", "old_log", "critic_values", "canonical_progress",
                "phases", "win_rewards", "terminals", "characters", "versions", "potentials",
            )
        } | {
            "sampled_length": 5, "sampled_positions": [1, 3],
            "terminal": True, "won": True,
        }
        compact = train.ExperienceDataset()
        self.assertEqual(compact.add({"trajectories": [compressed]}, Namespace(
            critic_only=True, gae_gamma=.5, gae_lambda=.5,
        )), counts)
        self.assertEqual(compact.rows, dataset.rows)
        for key in dataset.data:
            np.testing.assert_array_equal(
                compact.data[key][:len(compact)], dataset.data[key][:len(dataset)], err_msg=key,
            )

    def test_fifo_capacity_uses_arrival_order(self):
        def trajectory(versions):
            length = len(versions)
            return {
                "rows": [packed_row() for _ in versions], "choices": [0] * length,
                "old_log": [0] * length, "critic_values": [0] * length,
                "canonical_progress": [0] * length, "phases": [0] * length,
                "win_rewards": [0] * length,
                "terminals": [False] * (length - 1) + [True],
                "characters": [0] * length, "versions": versions,
                "potentials": [0] * length, "terminal_floor": versions[0],
            }

        dataset = train.ExperienceDataset()
        dataset.add({"trajectories": [trajectory([9, 9, 9]), trajectory([0, 0])]},
                    Namespace(gae_gamma=1, gae_lambda=1))
        np.testing.assert_array_equal(dataset.data["trajectory"][:5], [0, 0, 0, 1, 1])
        values = {key: value[:len(dataset)] for key, value in dataset.data.items()}
        stats = dataset.trajectory_stats(
            values, 10, values["trajectory"] == 0, [.1, -.2, .3, .4, -.5],
        )
        self.assertEqual(
            (stats["count"], stats["sampled_rows"], stats["floor_mean"]), (1, 3, 9),
        )
        self.assertEqual((stats["length_mean"], stats["policy_age_mean"]), (3, 1))
        self.assertEqual(stats["log_ratio"]["count"], 3)
        self.assertAlmostEqual(stats["log_ratio"]["mean"], .2 / 3)
        self.assertAlmostEqual(stats["log_ratio"]["abs_mean"], .2)
        self.assertAlmostEqual(stats["log_ratio"]["abs_p90"], .28)

        self.assertEqual(dataset.discard_ids([1]), 1)
        self.assertEqual(dataset.trim(2), 2)
        self.assertEqual(list(dataset.index), [3, 4])
        np.testing.assert_array_equal(dataset.data["trajectory"][:2], [1, 1])
        np.testing.assert_array_equal(dataset.data["version"][:2], [0, 0])
        np.testing.assert_array_equal(np.sort(dataset.data["id"][:2]), [3, 4])
        np.testing.assert_array_equal(
            np.sort(dataset.sample(2, np.random.default_rng(1), candidates=[0, 1])),
            [0, 1],
        )
        np.testing.assert_array_equal(
            dataset.sample(1, np.random.default_rng(1), True, candidates=[1]), [1],
        )
        self.assertEqual(dataset.discard_trajectories([1]), 2)
        self.assertEqual(len(dataset), 0)

    def test_forced_terminal_chain_ends_the_last_decision(self):
        trajectory = {
            "rows": [packed_row(2), packed_row(1), packed_row(1)],
            "choices": [0] * 3, "old_log": [0] * 3, "critic_values": [0] * 3,
            "canonical_progress": [0, 4, 7], "phases": [0] * 3,
            "win_rewards": [0] * 3, "terminals": [False, False, True],
            "characters": [0] * 3, "versions": [0] * 3, "potentials": [0] * 3,
            "terminal_floor": 15,
        }
        dataset = train.ExperienceDataset()
        counts = dataset.add({"trajectories": [trajectory]}, Namespace(
            critic_only=False, gae_gamma=1, gae_lambda=.995,
        ))

        expected = 7 / (train.CATEGORIES - 1)
        self.assertEqual(counts, (3, 2, 0))
        self.assertEqual(len(dataset), 1)
        self.assertEqual(dataset.data["terminal"][0], 7)
        self.assertEqual(dataset.data["trajectory_floor"][0], 15)
        self.assertEqual(dataset.data["trajectory_length"][0], 3)
        self.assertEqual(dataset.data["trajectory_actionable_length"][0], 1)
        self.assertAlmostEqual(dataset.data["advantage"][0], expected)
        self.assertAlmostEqual(dataset.data["critic_target"][0], expected)
        self.assertAlmostEqual(dataset.data["priority"][0], 5 + expected)
        stats = dataset.trajectory_stats(
            {key: value[:len(dataset)] for key, value in dataset.data.items()}, 2,
        )
        self.assertEqual(stats, {
            "count": 1, "sampled_rows": 1, "floor_counts": {"15": 1},
            "floor_mean": 15., "floor_median": 15., "floor_p90": 15., "floor_max": 15,
            "length_mean": 3., "length_max": 3,
            "actionable_length_mean": 1., "actionable_length_max": 1,
            "policy_span_mean": 0., "policy_span_max": 0,
            "policy_age_mean": 2., "policy_age_max": 2,
            "character_counts": [1, 0, 0, 0, 0],
        })


class ModelKernelTest(unittest.TestCase):
    def test_card_context_ignores_device_padding(self):
        env = train.sts2_sim.Batch(1, 7, 0)
        target = torch.device("mps") if torch.backends.mps.is_available() else torch.device("cpu")
        agent = model.Agent(dict(env.token_layout(64, 1, 4, 128)), 64, 1, 4, 128).to(target)
        values = torch.randn(510, 64, device=target)
        metadata = torch.zeros(512, 4, dtype=torch.long, device=target)
        metadata[:510, 3] = 1
        self.assertEqual(agent._card_context(values, metadata).shape, values.shape)

    def test_native_sequence_deduplication_preserves_empty_groups(self):
        selected, groups, inverse = train.sts2_sim.deduplicate_sequences(
            np.array([5, 7, 5, 8]), np.array([0, 1, 0, 1, 2, 3]),
            np.array([0, 2, 4, 4, 6]),
        )
        np.testing.assert_array_equal(selected, [0, 1, 2, 3])
        np.testing.assert_array_equal(groups, [0, 0, 2, 2])
        np.testing.assert_array_equal(inverse, [0, 0, 1, 2])

    def test_compact_training_unpack_preserves_features(self):
        env = train.sts2_sim.Batch(4, 7, None, ascension=0)
        rows = list(model._pack_batch(env.observe_tokens(flat=True)))
        normal = train.sts2_sim.unpack_packed_observations(rows)
        compact = train.sts2_sim.unpack_packed_observations(rows, True)
        for full, reduced in zip(normal[2], compact[2]):
            for index in (0, 4, 5, 7):
                np.testing.assert_array_equal(full[index], reduced[index])
            np.testing.assert_array_equal(full[2][full[6]], reduced[2])
            np.testing.assert_array_equal(full[3][full[6]], reduced[3])

    def test_precomputed_sequence_layout(self):
        groups = np.array([2, 0, 2, 1, 0, 2, 1])
        offsets, destination, sequence, _block_rows, _block_starts, \
            _metal_offsets, lengths, sparse = model.sequence_layout(
                groups, 3, 3, torch.device("cpu"),
            )
        positions, items, query_offsets = sparse
        np.testing.assert_array_equal(sequence[destination], groups)
        self.assertEqual(len(sequence) % 16_384, 0)
        np.testing.assert_array_equal(lengths[:3], np.bincount(groups) + 1)
        np.testing.assert_array_equal(positions[query_offsets[:3]], offsets[:3])
        np.testing.assert_array_equal(positions[items], destination[-3:])

    def test_shared_prefix_gru_preserves_outputs_and_gradients(self):
        tokens = np.array([1, 2, 3, 1, 2, 4, 1, 2], np.int64)
        counts = np.array([3, 3, 2, 0])
        (sources, parents, terminal), sizes = model.prefix_tree(tokens, counts)
        torch.manual_seed(0)
        embedding = torch.nn.Embedding(5, 4).double()
        gru = torch.nn.GRU(4, 4, batch_first=True).double()
        tree_embedding = torch.nn.Embedding(5, 4).double()
        cell = torch.nn.GRUCell(4, 4).double()
        tree_embedding.load_state_dict(embedding.state_dict())
        cell.load_state_dict({
            name: value for name, value in gru.state_dict().items()
            for name in (name.removesuffix("_l0"),)
        })
        sequences = np.split(tokens, np.cumsum(counts)[:-1])
        expected = torch.cat([
            gru(embedding(torch.as_tensor(sequence))[None])[1]
            for sequence in sequences if len(sequence)
        ], 1)[0]
        values = tree_embedding(torch.as_tensor(tokens[sources]))
        levels = []
        offset = 0
        for size in sizes:
            previous = values.new_zeros((size, 4)) if not levels \
                else levels[-1][torch.as_tensor(parents[offset:offset + size])]
            levels.append(cell(values[offset:offset + size], previous))
            offset += size
        actual = torch.cat(levels)[torch.as_tensor(terminal)]
        weight = torch.arange(actual.numel(), dtype=torch.double).view_as(actual)
        (expected * weight).sum().backward(); (actual * weight).sum().backward()
        torch.testing.assert_close(actual, expected)
        torch.testing.assert_close(tree_embedding.weight.grad, embedding.weight.grad)
        for name, parameter in cell.named_parameters():
            torch.testing.assert_close(parameter.grad, getattr(gru, name + "_l0").grad)

    @unittest.skipUnless(torch.backends.mps.is_available(), "MPS unavailable")
    def test_mps_linear_backward_input_matches_aten(self):
        torch.manual_seed(0)
        for output in (128, 384):
            value = torch.randn(4097, 128, dtype=torch.bfloat16, device="mps")
            weight = torch.randn(output, 128, dtype=torch.bfloat16, device="mps")
            grad = torch.randn(4097, output, dtype=torch.bfloat16, device="mps")
            expected = torch.ops.aten.linear_backward(
                value, grad, weight, [True, True, True],
            )
            actual = model._linear_backward(value, weight, grad)
            self.assertTrue(all(torch.equal(left, right)
                                for left, right in zip(actual, expected)))


class TrajectoryTest(unittest.TestCase):
    @staticmethod
    def trace():
        env = train.sts2_sim.Batch(1, 7, 0, ascension=0)
        env.observe_tokens()
        actions = env.action_descriptors()
        choice = 0
        return {
            "trajectory_schema": 1, "id": "trace", "seed": 7, "character": 0,
            "stage": 0, "ascension": 0, "bonus": 0,
            "temperature": .8, "fingerprint": env.fingerprint(),
            "choices": [choice],
            "log_policies": [[-math.log(len(actions))] * len(actions)],
            "critic_values": [.25], "canonical_progress": [0], "phases": [0],
            "policy_revisions": [3], "terminal_floor": 0, "terminal": False,
            "outcome": "step_cap",
        }

    def test_trace_replays_state_and_action_descriptors(self):
        replay = trajectories.replay_trace(self.trace())
        self.assertEqual(len(replay["steps"]), 1)
        self.assertEqual(replay["steps"][0]["choice"], 0)
        state = replay["steps"][0]["state"]
        self.assertEqual(state["common"]["character"], "CHARACTER.IRONCLAD")
        event = state["event"]
        self.assertEqual((event["id"], event["page"]), ("EVENT.NEOW", "initial"))
        self.assertTrue(event["options"])
        action = replay["steps"][0]["actions"][0]
        self.assertEqual((action["kind"], action["target"]), ("event_option", "option:0"))
        self.assertIsNotNone(action["label"])
        self.assertAlmostEqual(sum(
            action["probability"] for action in replay["steps"][0]["actions"]
        ), 1)
        self.assertAlmostEqual(
            replay["steps"][0]["policy_entropy"], math.log(len(replay["steps"][0]["actions"])),
        )
        self.assertAlmostEqual(
            replay["steps"][0]["effective_actions"], len(replay["steps"][0]["actions"]),
        )

    def test_replay_exposes_potential_components_and_environment_reward(self):
        weights = [(index - 6) * 1e-7 for index in range(len(train.POTENTIAL_TERMS))]
        config = trajectories._training_config({
            "architecture": {"potential_weights": weights},
        }, self.trace())
        replay = trajectories.replay_trace(self.trace(), config)
        step = replay["steps"][0]
        env = train.sts2_sim.Batch(1, 7, 0, ascension=0)
        env.set_potential_weights(weights)
        expected = float(env.observe_tokens()[4][0])
        self.assertAlmostEqual(step["potential"], expected)
        self.assertAlmostEqual(
            step["potential_floor"] + sum(
                row["contribution"] for row in step["potential_components"]
            ), expected,
        )
        self.assertEqual(len(step["potential_components"]), len(train.POTENTIAL_TERMS))
        self.assertEqual(step["potential_health"], 80)
        self.assertEqual(step["potential_components"][0]["feature"], 1)
        np.testing.assert_allclose(
            [row["feature"] for row in step["potential_components"][:7]],
            [1, 80, 0, 1, 0, 99, 10],
        )
        np.testing.assert_allclose([
            row["weighted_feature"] * step["potential_health"]
            for row in step["potential_components"]
        ], [row["contribution"] for row in step["potential_components"]], atol=1e-9)
        self.assertEqual(step["environment_reward"], 0)

    def test_training_signal_matches_actionable_gae_sequence(self):
        steps = [
            {"actions": [{}, {}], "critic_value": .1, "potential": .2,
             "environment_reward": 0., "policy_entropy": .5, "choice": 0,
             "potential_components": []},
            {"actions": [{}], "critic_value": .8, "potential": .4,
             "environment_reward": 0., "policy_entropy": 0., "choice": 0,
             "potential_components": []},
            {"actions": [{}, {}], "critic_value": .2, "potential": .3,
             "environment_reward": 0., "policy_entropy": .4, "choice": 1,
             "potential_components": []},
        ]
        for step in steps:
            for action in step["actions"]:
                action["log_probability"] = -math.log(len(step["actions"]))
        trace = {"terminal": True, "outcome": "dead", "canonical_progress": [10, 11, 12]}
        config = {"gae_gamma": .9, "gae_lambda": .8,
                  "boss_floor_increments": list(trajectories.BOSS_FLOOR_INCREMENTS)}
        trajectories._add_training_signals(trace, steps, config)
        terminal = 12 / (train.CATEGORIES - 1)
        last_reward = terminal - .3
        first_reward = .9 * .3 - .2
        last_delta = last_reward - .2
        first_delta = first_reward + .9 * .2 - .1
        self.assertAlmostEqual(steps[2]["training_signal"]["return"], last_reward)
        self.assertAlmostEqual(steps[0]["training_signal"]["return"],
                               first_reward + .9 * last_reward)
        self.assertAlmostEqual(steps[0]["training_signal"]["advantage"],
                               first_delta + .9 * .8 * last_delta)
        self.assertEqual(steps[0]["training_signal"]["next_actionable_index"], 2)
        self.assertIsNone(steps[2]["training_signal"]["next_actionable_index"])
        self.assertFalse(steps[1]["training_signal"]["eligible"])
        self.assertIn("forced choice", steps[1]["training_signal"]["reason"])

    def test_training_signal_terminal_values_match_floor_rewards(self):
        config = {"boss_floor_increments": [12., 13., 14.]}
        denominator = train.CATEGORIES - 1
        args = Namespace(boss_1_floor_increment=12, boss_2_floor_increment=13,
                         boss_3_floor_increment=14)
        for outcome, canonical, category, won in (
            ("dead", [54], 54, False), ("won", [0], train.CATEGORIES - 1, True),
        ):
            self.assertAlmostEqual(trajectories._terminal_value({
                "outcome": outcome, "canonical_progress": canonical,
            }, config), train.terminal_floor(category, won, args) / denominator)

    def test_training_signal_matches_experience_dataset(self):
        legal = [2, 1, 3, 2, 1]
        predicted = [.11, .8, -.2, .3, .9]
        potentials = [.4, .8, -.1, .2, .7]
        canonical = [24, 25, 53, 54, 55]
        trajectory = {
            "rows": [packed_row(count) for count in legal], "choices": [0] * 5,
            "old_log": [-math.log(count) for count in legal],
            "critic_values": predicted, "canonical_progress": canonical,
            "phases": [0] * 5, "win_rewards": [0] * 5,
            "terminals": [False] * 4 + [True], "characters": [0] * 5,
            "versions": [0] * 5, "potentials": potentials,
        }
        args = Namespace(critic_only=False, gae_gamma=.9, gae_lambda=.8,
                         boss_1_floor_increment=12, boss_2_floor_increment=13,
                         boss_3_floor_increment=14)
        dataset = train.ExperienceDataset()
        dataset.add({"trajectories": [trajectory]}, args)
        steps = [{
            "actions": [{"log_probability": -math.log(count)} for _ in range(count)],
            "choice": 0, "critic_value": value, "potential": potential,
            "potential_components": [], "environment_reward": 0.,
            "policy_entropy": math.log(count),
        } for count, value, potential in zip(legal, predicted, potentials)]
        config = {"gae_gamma": args.gae_gamma, "gae_lambda": args.gae_lambda,
                  "boss_floor_increments": [12., 13., 14.]}
        trajectories._add_training_signals({
            "terminal": True, "outcome": "dead", "canonical_progress": canonical,
        }, steps, config)
        signals = [step["training_signal"] for step in steps if len(step["actions"]) > 1]
        count = len(signals)
        np.testing.assert_allclose(
            [row["advantage"] for row in signals], dataset.data["advantage"][:count],
            atol=1e-7,
        )
        np.testing.assert_allclose(
            [row["return"] for row in signals], dataset.data["critic_target"][:count],
            atol=1e-7,
        )
        np.testing.assert_allclose(
            [row["critic_value"] for row in signals], dataset.data["value"][:count],
            atol=1e-7,
        )
        targets = dataset.data["critic_target"][:count]
        rewards = targets - args.gae_gamma * np.append(targets[1:], 0.)
        np.testing.assert_allclose([row["reward"] for row in signals], rewards, atol=1e-7)
        np.testing.assert_allclose([
            row["td_error"] for row in signals
        ], rewards + args.gae_gamma * np.append(dataset.data["value"][1:count], 0.)
           - dataset.data["value"][:count], atol=1e-7)

    def test_training_config_uses_matching_session_or_latest(self):
        manifest = {"sessions": [
            {"id": 1, "training": {"gae_lambda": .5}},
            {"id": 2, "training": {"gae_lambda": .25}},
        ]}
        self.assertEqual(trajectories._training_config(
            manifest, {"session_id": 1},
        )["gae_lambda"], .5)
        self.assertEqual(trajectories._training_config(manifest, {})["gae_lambda"], .25)

    def test_native_policy_details_match_selected_probability(self):
        env = train.sts2_sim.Batch(2, 7, 0, ascension=0)
        model = train.Agent(
            dict(env.token_layout(*train.DEFAULT_ARCHITECTURE)), *train.DEFAULT_ARCHITECTURE,
        )
        env.load_policy(train.export_value_model(None, model, env.fingerprint(), 1, 0, True))
        _, policy, critic = env.policy_details([0], .8)[0]
        result = env.policy(.8, False, False)
        repeated = env.policy(.8, False, False)
        for left, right in zip(result[:4], repeated[:4]):
            np.testing.assert_array_equal(left, right)
        self.assertAlmostEqual(sum(math.exp(value) for value in policy), 1, places=5)
        self.assertAlmostEqual(policy[int(result[1][0])], float(result[2][0]), places=5)
        self.assertTrue(math.isfinite(critic))

    def test_potential_modes(self):
        env = train.sts2_sim.Batch(1, 7, 0, ascension=0)
        weights = [1e-4] * len(train.POTENTIAL_TERMS)
        env.set_potential_weights(weights)
        multiplied = float(env.observe_tokens()[4][0])
        env.set_potential_mode("additive")
        additive = float(env.observe_tokens()[4][0])
        self.assertGreater(multiplied, additive)
        with self.assertRaises(ValueError):
            env.set_potential_mode("invalid")

        model = train.Agent(
            dict(env.token_layout(*train.DEFAULT_ARCHITECTURE)), *train.DEFAULT_ARCHITECTURE,
            potential_weights=weights, potential_mode="additive",
        )
        self.assertEqual(train.architecture(model)["potential_mode"], "additive")
        with self.assertRaises(ValueError):
            train.Agent(
                dict(env.token_layout(*train.DEFAULT_ARCHITECTURE)),
                *train.DEFAULT_ARCHITECTURE, model_version=train.MODEL_VERSION - 1,
            )
        env.set_potential_mode("health_multiplier")
        env.load_policy(train.export_value_model(None, model, env.fingerprint(), 1, 0, True))
        self.assertAlmostEqual(float(env.observe_tokens()[4][0]), additive)

    def test_explanation_deletes_tokens_and_captures_attention(self):
        env = train.sts2_sim.Batch(1, 1, None, ascension=0)
        model = train.Agent(
            dict(env.token_layout(*train.DEFAULT_ARCHITECTURE)), *train.DEFAULT_ARCHITECTURE,
        )
        model.eval()
        policy = train.export_value_model(None, model, env.fingerprint(), 1, 0, True)
        trace = trajectories.TrajectorySource._generate_one(
            policy, env.fingerprint(), "memory", 0, 123, 0, 0, 0, 0, .8, False, 1, 1,
        )
        replay = train.sts2_sim.Batch(1, trace["seed"], 0, ascension=0)
        observation = replay.observe_tokens(flat=True)
        result = explain(
            model, observation, json.loads(replay.state(0)), replay.action_descriptors(), trace, 0,
        )
        self.assertTrue(any(row.get("removed_tokens", 0) for row in result["influences"]))
        self.assertIn("global_transformer", {row["hierarchy"] for row in result["attention"]})
        self.assertTrue(all(abs(sum(row) - 1) < 1e-5
                            for record in result["attention"]
                            for head in record["weights"] for row in head))
        case = next(row["id"] for row in result["influences"]
                    if row["status"] != "unavailable")
        compared = explain(
            model, observation, json.loads(replay.state(0)), replay.action_descriptors(),
            trace, 0, case,
        )
        self.assertEqual(compared["attention_case"], case)
        self.assertTrue(compared["case_attention"])

    def test_card_actions_have_names_and_previews(self):
        env = train.sts2_sim.Batch(1, 1, 0, teacher_width=8, teacher_turns=1, ascension=0)
        env.set_training_bonus(24)
        found = set()
        for _ in range(800):
            env.observe_tokens()
            descriptors = env.action_descriptors()
            rows = [(descriptor, json.loads(preview) if preview else None)
                    for descriptor, preview in zip(descriptors, env.action_previews())]
            if {"reward_card", "reward_potion"} <= {row[0] for row in descriptors}:
                card = next(row for row in rows if row[0][0] == "reward_card")
                potion = next(row for row in rows if row[0][0] == "reward_potion")
                self.assertTrue(card[0][3].startswith("CARD."))
                self.assertEqual(card[1]["kind"], "card")
                self.assertTrue(card[1]["effects"])
                self.assertTrue(potion[0][3].startswith("POTION."))
                self.assertEqual(potion[1]["kind"], "potion")
                found.add("reward")
            removed = next((row for row in rows
                            if row[0][0].startswith("RemoveCard(")), None)
            if removed:
                self.assertEqual(removed[1]["kind"], "card")
                self.assertTrue(removed[1]["id"].startswith("CARD."))
                found.add("remove")
            if len(found) == 2:
                break
            env.step(env.teacher())
        else:
            self.fail("teacher did not reach card reward and removal choices")

    def test_trace_catalog_reads_gzip_and_ignores_malformed_records(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; events = run / "events"
            events.mkdir(parents=True)
            (run / "run.json").write_text(json.dumps({
                "sessions": [{"training": {"policy_temperature": .7}}],
            }))
            with gzip.open(events / "000001.jsonl.gz", "wt") as output:
                output.write("not json\n")
                output.write(json.dumps({"event": "trajectory"} | self.trace()) + "\n")
            source = trajectories.TrajectorySource(root)
            listing = source.list("run")
            self.assertEqual(listing["defaults"]["temperature"], .7)
            self.assertEqual([row["id"] for row in listing["traces"]], ["trace"])
            imported = source.add("run", json.loads(json.dumps(self.trace())))
            self.assertEqual(imported["trace"]["source"], "imported")
            self.assertEqual(len(imported["steps"]), 1)
            with self.assertRaisesRegex(ValueError, "invalid run"):
                source.list("../run")

    def test_catalog_prefers_compatible_runs_and_disables_old_checkpoints(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, version in (("old", train.MODEL_VERSION - 1),
                                  ("current", train.MODEL_VERSION)):
                run = root / name
                run.mkdir()
                (run / "run.json").write_text(json.dumps({"model_version": version}))
            source = trajectories.TrajectorySource(root)
            self.assertEqual(source.catalog()[0], "current")
            self.assertIn("requires model", source.list("old")["checkpoint_error"])
            self.assertIsNone(source.list("current")["checkpoint_error"])

    def test_capture_disabled_does_not_inspect_environments(self):
        collector = train.RolloutCollector.__new__(train.RolloutCollector)
        collector.args = Namespace(capture_trajectories=False)
        collector.env = mock.Mock()
        self.assertEqual(collector.trace_indices(), [])
        collector.env.characters.assert_not_called()

    def test_native_long_trajectory_compaction_preserves_samples(self):
        args = train.parser().parse_args(["train"])
        args.envs = 16
        args.samplers = 1
        args.cache_features = False
        env = train.sts2_sim.Batch(1, 0, None, ascension=0)
        shape = (64, 2, 4, 128)
        model = train.Agent(dict(env.token_layout(*shape)), *shape)
        actor = train.export_value_model(None, model, env.fingerprint(), 1, 0, True)

        results = []
        for window in (10 ** 9, 4):
            with mock.patch.object(rollout, "NATIVE_HISTORY_WINDOW", window):
                collector = train.RolloutCollector(
                    args, 1, 0, train.WinningReservoir(0, args.envs),
                )
                collector.env.load_policy(actor)
                results.append(collector.collect(
                    None, None, "fp32", float("inf"), 200, 17,
                ))
                self.assertLessEqual(len(collector.native_steps), window * 2)
                for prefixes, length in zip(collector.native_prefixes, collector.episode_steps):
                    self.assertLessEqual(len(prefixes), int(length) // window + 1)

        original, compacted = results
        self.assertEqual(len(original["trajectories"]), len(compacted["trajectories"]))
        self.assertGreater(len(original["trajectories"]), 0)
        for left, right in zip(original["trajectories"], compacted["trajectories"]):
            self.assertGreaterEqual(left["sampled_length"], len(left["rows"]))
            self.assertEqual(len(left["sampled_positions"]), len(left["rows"]))
            self.assertTrue(all(train.packed_legal_count(row) > 1 for row in left["rows"]))
            for key in left:
                if key == "completion_seconds":
                    continue
                if key == "rows":
                    self.assertEqual(left[key], right[key])
                else:
                    np.testing.assert_array_equal(left[key], right[key], err_msg=key)
        for left, right in zip(original["completions"], compacted["completions"]):
            self.assertEqual(left | {"completion_seconds": 0},
                             right | {"completion_seconds": 0})

    def test_capture_slots_are_balanced_by_character(self):
        collector = train.RolloutCollector.__new__(train.RolloutCollector)
        collector.args = Namespace(capture_trajectories=True, policy_temperature=.8)
        collector.env = mock.Mock()
        collector.env.characters.return_value = [0, 1, 2, 3, 4, 0]
        collector.env.seeds.return_value = [10, 11, 12, 13, 14, 15]
        collector.env.fingerprint.return_value = 9
        collector.episode_steps = np.zeros(6, np.int32)
        collector.traces = [None] * 6
        collector.sampler_session = 1; collector.worker = 2
        collector.generation = 3; collector.iteration = 4; collector.stage = 0
        self.assertEqual(collector.trace_indices(), [0, 1, 2, 3, 4])
        self.assertEqual([collector.traces[index]["character"] for index in range(5)],
                         list(range(5)))

    def test_trajectory_page_has_replay_and_portability_controls(self):
        page = trajectories.trajectory_page()
        for text in ("Generate from checkpoint", "Download JSON", "Import", "Legal actions",
                     "Explain decision", "Causal token deletion",
                     "What the training signal would be if this was a training example"):
            self.assertIn(text, page)
        self.assertIn("katex@0.16.11", page)
        self.assertIn("aria-expanded", page)
        self.assertIn("replaceAll('\\\\left','').replaceAll('\\\\right','')", page)
        self.assertIn("!seen.has(item.id)", page)
        self.assertIn("function returnTerm(row,offset)", page)
        self.assertIn("function advantageTerm(row,offset)", page)
        self.assertIn("\\\\mathrm{HP}(s_{${at}})", page)
        self.assertIn("item.weighted_feature", page)
        for formula in ("\\\\ell_t^{PPO}", "c_V\\\\ell_t^V", "\\\\widehat A_t",
                        "\\\\rho_t", "\\\\operatorname{clip}",
                        "\\\\delta_{${at}}", "G_{${at}}", "r_{${at}}",
                        "\\\\sum_k w_kx_k"):
            self.assertIn(formula, page)
        self.assertIn("definition:[policy,'+',critic,'-',entropyWeight", page)
        self.assertIn("typeof node.definition==='function'", page)
        self.assertIn("toExponential(5)", page)
        self.assertIn("action.kind==='use_potion'?`Drink ${name}`", page)
        self.assertIn("action.kind==='discard_potion'?`Discard ${name}`", page)
        self.assertIn("action.kind.startsWith('RemoveCard(')?`${action.kind} (${name})`", page)
        self.assertIn("${actionThing(action)}", page)
        self.assertIn("text.match(/<p>Message: ([^<]+)/)", page)
        self.assertIn("id=generationNote", page)
        self.assertIn("renderTraining(terminal?null:row)", page)
        self.assertIn("row.id?title(row)", page)
        self.assertIn("/^(CARD|RELIC|POTION|POWER|ORB)\\./", page)
        self.assertIn("action-name:hover>.preview,.item:hover>.preview", page)
        self.assertIn(".preview{display:none", page)
        self.assertIn(".preview-description{display:block", page)
        self.assertIn("<span class=preview-description>${plain(text,value)}</span>", page)
        self.assertIn("<span class=preview-description>${rich(description(row),row)}</span>", page)
        self.assertNotIn("<p>${plain(text,value)}</p>", page)
        self.assertNotIn("<p>${rich(description(row),row)}</p>", page)
        self.assertIn("request('/game-assets/assets.json')", page)
        self.assertIn("request('/game-assets/glossary.json')", page)
        self.assertIn("background-image:url('/game-assets/", page)
        self.assertIn("class=card-description", page)
        self.assertIn("resolved legal choice", page)
        self.assertNotIn("event data [", page)
        self.assertLess(page.index("id=phase"), page.index("Legal actions"))
        self.assertGreater(page.index("id=inventory"), page.index("Legal actions"))

    def test_trajectory_assets_include_game_card_and_relic_art(self):
        root = Path(trajectories.__file__).with_name("game-assets")
        assets = json.loads((root / "assets.json").read_text())
        self.assertEqual(assets["card/bash"][3:], [250, 190])
        self.assertEqual(assets["relic/burning_blood"][3:], [71, 81])
        self.assertEqual(assets["potion/energy_potion"][3:], [80, 80])
        self.assertEqual(assets["power/vulnerable_power"][3:], [64, 64])
        glossary = json.loads((root / "glossary.json").read_text())
        self.assertEqual(glossary["card"]["BASH.title"], "Bash")
        self.assertIn("end of combat", glossary["relic"]["BURNING_BLOOD.description"])
        self.assertEqual(
            glossary["content"]["relics"]["RELIC.LEAFY_POULTICE"]["values"]["MaxHp"],
            12,
        )
        for atlas in {row[0] for row in assets.values()}:
            data = (root / f"{atlas}.webp").read_bytes()
            self.assertEqual((data[:4], data[8:12]), (b"RIFF", b"WEBP"))

    def test_game_content_previews_expose_hover_details(self):
        content = json.loads((Path(trajectories.__file__).with_name(
            "game-assets"
        ) / "glossary.json").read_text())["content"]
        self.assertEqual(content["cards"]["CARD.BASH"]["cost"], [2, 2])
        self.assertEqual(content["potions"]["POTION.ENERGY_POTION"]["target"], "Player")
        self.assertTrue(content["powers"]["POWER.VULNERABLE_POWER"]["debuff"])


if __name__ == "__main__":
    unittest.main()
