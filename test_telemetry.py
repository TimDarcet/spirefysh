import json
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path

import numpy as np
import torch

import train


def write_events(path, rows):
    with path.open("ab") as output:
        for row in rows:
            output.write((json.dumps({"event_schema": 2} | row) + "\n").encode())


def packed_row(legal=2):
    return (0, (), (), (), np.ones((legal, 1), np.uint32), 0)


class TelemetryTest(unittest.TestCase):
    def test_training_performance_controls(self):
        args = train.parser().parse_args([
            "train", "--disable-post-kl-check", "--mps-empty-cache-updates", "8",
        ])
        self.assertTrue(args.disable_post_kl_check)
        self.assertEqual(args.mps_empty_cache_updates, 8)
        self.assertEqual(args.batch, 16_384)
        self.assertEqual(args.max_policy_lag, 20)
        self.assertEqual(args.gae_gamma, 1)
        self.assertEqual(args.gae_lambda, .99)
        self.assertEqual(args.learning_rate_warmup_steps, 200)

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
                 "packet_sampled_decisions": 80, "admitted_rows": 30,
                 "discarded_decisions": 5, "forced_rows": 3,
                 "budget_excess_rows": 2, "collect_seconds": 2,
                 "episodes": [{"character": 2, "floor": 12, "won": True,
                               "step_cap": False, "combat_cap": False,
                               "empty_actions": False, "length": 20,
                               "completion_seconds": 1, "iteration": 5}]},
                {"event": "training_batch", "time": 3, "step": 40, "stage": 0,
                 "weights_revision": 1, "policy_revision": 1,
                 "attempted_rows": 20, "fresh_rows": 20,
                 "policy_trained_rows": 20, "critic_trained_rows": 20,
                 "policy_outcome": "accepted", "commit_kind": "full",
                 "retired_rows": 4,
                 "policy_loss": 2, "critic_loss": 3,
                 "search_consistency_loss": .25, "gradient_clipped": True,
                 "critic_explained_reward_variance": .5, "total_seconds": 1,
                 "critic_explained_reward_variance_by_floor": {
                     "12": {"value": .5, "target_variance": .25, "rows": 20}},
                 "training_elapsed_seconds": 3},
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
            self.assertEqual(metrics["policy_loss"], 2)
            self.assertEqual(metrics["critic_explained_reward_variance"], .5)
            self.assertEqual(metrics["critic_explained_reward_variance_by_floor"], {
                "12": {"value": .5, "target_variance": .25, "rows": 20},
            })
            self.assertEqual(metrics["critic_floor_conditioned_explained_reward_variance"], .5)
            self.assertEqual(metrics["search_consistency_loss"], .25)
            self.assertEqual(metrics["gradient_clipped"], 1)
            self.assertEqual(metrics["dataset_rows"], 7)
            self.assertEqual(metrics["optimizer_steps_per_second"], 1)
            self.assertEqual(metrics["used_rows_per_second"], 20)
            self.assertEqual(metrics["optimizer_steps"], [{
                "step": 40, "weights_revision": 1, "seconds": 3,
                "optimizer_steps_per_second": 1,
                "used_rows_per_second": 20, "total_seconds": 1,
            }])
            self.assertEqual(metrics["dataset_rollout_dropped"], 3)
            self.assertEqual(metrics["dataset_budget_dropped"], 2)
            self.assertEqual(metrics["dataset_forced_dropped"], 3)
            self.assertEqual(metrics["dataset_retired"], 4)

    def test_projector_tracks_policy_rejections(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root / "run"; (run / "events").mkdir(parents=True)
            manifest = self.manifest("run", 1)
            (run / "run.json").write_text(json.dumps(manifest))
            write_events(run / "events/000001.jsonl", [
                {"event": "training_batch", "time": 1, "step": 40, "stage": 0,
                 "policy_outcome": "pre_kl_rejected", "commit_kind": "critic_only",
                 "fresh_rows": 5, "critic_trained_rows": 5,
                 "critic_explained_reward_variance": .2,
                 "critic_explained_reward_variance_by_floor": {
                     "10": {"value": .2, "target_variance": .25, "rows": 5}},
                 "total_seconds": 2},
                {"event": "training_batch", "time": 2, "step": 40, "stage": 0,
                 "policy_outcome": "post_kl_rejected", "commit_kind": "critic_only",
                 "fresh_rows": 7, "critic_trained_rows": 7,
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
            train.dashboard(root)
            html = (root / "dashboard.html").read_text()
            self.assertIn('"step":0', html)
            self.assertFalse((run / "live.js").exists())
            self.assertTrue(all(label in html for label in (
                "Lineage", "Branch", "Compare", "Metrics", "Subsample", "Weights revision", "Policy loss", "Gradient norm",
                "Optimizer steps / second", "Used rows / second", "Optimizer step time",
                "Policy lag (p95)",
                "Row outcomes",
            )))
            self.assertIn("weights_revision??report.metrics.updates", html)
            self.assertIn("series(reports,'advantage_mean','mean_advantage')", html)
            self.assertIn("optimizerSeries(steps,reports,'used_rows_per_second')", html)
            self.assertIn("function addComparison", html)
            self.assertIn("function clipComparison", html)
            self.assertIn("setFullRange([primary])", html)
            self.assertIn("compareSelect.onchange=()=>showVersion()", html)
            self.assertIn("id=legend class=legend", html)
            self.assertIn("plotly_relayout", html)
            self.assertIn("showlegend:false", html)
            self.assertIn("uirevision:uiRevision", html)
            self.assertIn("id=subsample type=number min=1 step=1 value=3", html)
            self.assertIn("appendReports(run,live.reports,live.optimizer_steps,index)", html)
            self.assertIn("critic_explained_reward_variance", html)
            self.assertIn("metricPicker.onchange", html)
            self.assertIn("</div><details class=panel open><summary>Metrics", html)
            self.assertNotIn("lines+markers", html)
            self.assertIn("Plotly.extendTraces", html)
            self.assertIn("fetch(url", html)
            self.assertIn("row.step>step", html)

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
                   potentials=None, field="advantage"):
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
            }
            dataset = train.ExperienceDataset()
            dataset.add({"trajectories": [trajectory]}, Namespace(
                critic_only=False, gae_gamma=gamma, gae_lambda=gae_lambda,
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

    def test_forced_terminal_chain_ends_the_last_decision(self):
        trajectory = {
            "rows": [packed_row(2), packed_row(1), packed_row(1)],
            "choices": [0] * 3, "old_log": [0] * 3, "critic_values": [0] * 3,
            "canonical_progress": [0, 4, 7], "phases": [0] * 3,
            "win_rewards": [0] * 3, "terminals": [False, False, True],
            "characters": [0] * 3, "versions": [0] * 3, "potentials": [0] * 3,
        }
        dataset = train.ExperienceDataset()
        counts = dataset.add({"trajectories": [trajectory]}, Namespace(
            critic_only=False, gae_gamma=1, gae_lambda=.995,
        ))

        expected = 7 / (train.CATEGORIES - 1)
        self.assertEqual(counts, (3, 2, 0))
        self.assertEqual(len(dataset), 1)
        self.assertEqual(dataset.data["terminal"][0], 7)
        self.assertAlmostEqual(dataset.data["advantage"][0], expected)
        self.assertAlmostEqual(dataset.data["critic_target"][0], expected)
        self.assertAlmostEqual(dataset.data["priority"][0], 5 + expected)


if __name__ == "__main__":
    unittest.main()
