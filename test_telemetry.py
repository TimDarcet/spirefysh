import json
import tempfile
import unittest
from pathlib import Path

import train


def write_events(path, rows):
    with path.open("ab") as output:
        for row in rows:
            output.write((json.dumps({"event_schema": 2} | row) + "\n").encode())


class TelemetryTest(unittest.TestCase):
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
                 "discarded_decisions": 5, "collect_seconds": 2,
                 "episodes": [{"character": 2, "floor": 12, "won": True,
                               "step_cap": False, "combat_cap": False,
                               "empty_actions": False, "length": 20,
                               "completion_seconds": 1, "iteration": 5}]},
                {"event": "training_batch", "time": 3, "step": 40, "stage": 0,
                 "weights_revision": 1, "policy_revision": 1,
                 "attempted_rows": 20, "fresh_rows": 20,
                 "policy_trained_rows": 20, "critic_trained_rows": 20,
                 "policy_loss": 2, "critic_loss": 3, "total_seconds": 1,
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
            self.assertEqual(metrics["dataset_rows"], 7)

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

    def test_dashboard_rebuilds_live_metrics_without_reports(self):
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
            live = train.read_live(run / "live.js")
            html = (root / "dashboard.html").read_text()
            self.assertEqual(live["reports"][0]["step"], 0)
            self.assertTrue(all(label in html for label in (
                "Lineage", "Branch", "Weights revision", "Policy loss", "Gradient norm",
            )))
            self.assertIn("weights_revision??report.metrics.updates", html)
            self.assertIn("row.step>step", html)

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


if __name__ == "__main__":
    unittest.main()
