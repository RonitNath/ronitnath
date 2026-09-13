import importlib.util
import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import Mock, patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


coordinator = load("coordinator")
cleanup = load("cleanup")
host_entry = load("host-entry")


class CoordinatorRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.release = {
            "releaseId": "10000000-0000-4000-8000-000000000001",
            "previousRuntimeDigest": "sha256:" + "a" * 64,
            "migrations": {"entries": [{"transactional": True}]},
            "compatibility": {
                "rollbackRead": True,
                "previousRuntime": {
                    "repository": "ghcr.io/ronitnath/ronitnath-app",
                    "sourceSha": "b" * 40,
                },
            },
        }

    def test_restart_probes_completed_migration_before_mutating(self):
        probe = subprocess.CompletedProcess([], 0, "complete", "")
        with patch.object(coordinator.subprocess, "run", return_value=probe), patch.object(
            coordinator, "checkpoint", return_value={"migration": "complete"}
        ) as checkpoint, patch.object(coordinator, "run") as mutation:
            result = coordinator.migrate(self.release, "migration-image", {"migration": "intent"})
        self.assertEqual(result["migration"], "complete")
        checkpoint.assert_called_once()
        mutation.assert_not_called()

    def test_restart_stops_on_unknown_nontransactional_migration(self):
        self.release["migrations"]["entries"][0]["transactional"] = False
        probe = subprocess.CompletedProcess([], 1, "", "partial")
        with patch.object(coordinator.subprocess, "run", return_value=probe), patch.object(
            coordinator,
            "checkpoint",
            return_value={"state": "degraded", "migration": "unknown"},
        ), patch.object(coordinator, "run") as mutation:
            result = coordinator.migrate(self.release, "migration-image", {"migration": "intent"})
        self.assertEqual(result["migration"], "unknown")
        mutation.assert_not_called()

    def test_rollback_changes_only_replicas_at_target_digest(self):
        target = "sha256:" + "c" * 64
        with patch.object(coordinator, "current_local", return_value={"digest": target}), patch.object(
            coordinator, "remote", return_value={"digest": self.release["previousRuntimeDigest"]}
        ) as remote, patch.object(coordinator, "roll_local") as local, patch.object(
            coordinator, "checkpoint"
        ):
            coordinator.rollback_changed(self.release, target, {"migration": "complete"}, RuntimeError("failed"))
        local.assert_called_once()
        self.assertEqual(remote.call_count, 1)
        self.assertEqual(remote.call_args.args[0], "replica-status")

    def test_checkpoint_is_durable_when_telemetry_is_offline(self):
        original = coordinator.STATE
        try:
            with tempfile.TemporaryDirectory() as directory:
                coordinator.STATE = Path(directory)
                with patch.object(coordinator, "post_events", side_effect=OSError("offline")):
                    value = coordinator.checkpoint(self.release, "deploying", "observed", migration="pending")
                self.assertEqual(value["step"], "observed")
                event = __import__("json").loads((Path(directory) / (self.release["releaseId"] + ".journal.jsonl")).read_text())
                self.assertEqual(event["type"], "coordinator-checkpoint")
                self.assertEqual(event["manifest"]["releaseId"], self.release["releaseId"])
                self.assertTrue((Path(directory) / (self.release["releaseId"] + ".telemetry.jsonl")).exists())
        finally:
            coordinator.STATE = original

    def test_telemetry_buffer_stays_within_limit_and_reports_gap(self):
        original_state = coordinator.STATE
        original_limit = coordinator.TELEMETRY_LIMIT
        try:
            with tempfile.TemporaryDirectory() as directory:
                coordinator.STATE = Path(directory)
                coordinator.TELEMETRY_LIMIT = 800
                for seq in range(20):
                    coordinator.buffer_event(self.release["releaseId"], {
                        "schemaVersion": 2,
                        "releaseId": self.release["releaseId"],
                        "producerId": "test",
                        "seq": seq,
                        "at": "2026-09-13T00:00:00Z",
                        "type": "coordinator-checkpoint",
                        "payload": "x" * 150,
                    })
                path = Path(directory) / (self.release["releaseId"] + ".telemetry.jsonl")
                self.assertLessEqual(path.stat().st_size, coordinator.TELEMETRY_LIMIT)
                events = [json.loads(line) for line in path.read_text().splitlines()]
                self.assertEqual(events[0]["type"], "telemetry-gap")
                self.assertEqual(events[-1]["seq"], 19)
        finally:
            coordinator.STATE = original_state
            coordinator.TELEMETRY_LIMIT = original_limit

    def test_cleanup_compacts_terminal_records_but_preserves_active_records(self):
        original = cleanup.STATE
        try:
            with tempfile.TemporaryDirectory() as directory:
                cleanup.STATE = Path(directory)
                now = time.time()
                terminal = self.release["releaseId"]
                active = "10000000-0000-4000-8000-000000000002"
                manifest = {
                    "requestedSha": "b" * 40,
                    "manifestDigest": "sha256:" + "c" * 64,
                    "artifacts": {"runtime": {"digest": "sha256:" + "d" * 64}},
                }
                for release_id, state in ((terminal, "deployed"), (active, "deploying")):
                    (cleanup.STATE / (release_id + ".state.json")).write_text(json.dumps({"releaseId": release_id, "state": state, "step": "finished", "updatedAt": "2026-01-01T00:00:00Z"}))
                    (cleanup.STATE / (release_id + ".manifest.json")).write_text(json.dumps(manifest))
                    os.utime(cleanup.STATE / (release_id + ".state.json"), (now - cleanup.DETAIL_SECONDS - 1,) * 2)
                cleanup.compact(now)
                self.assertFalse((cleanup.STATE / (terminal + ".state.json")).exists())
                self.assertTrue((cleanup.STATE / (terminal + ".summary.json")).exists())
                self.assertTrue((cleanup.STATE / (active + ".state.json")).exists())
        finally:
            cleanup.STATE = original

    def test_recovery_finishes_owned_release_without_rechecking_moving_branch(self):
        original = coordinator.STATE
        original_lock = coordinator.LOCK
        release_id = self.release["releaseId"]
        target = "sha256:" + "c" * 64
        self.release.update({
            "requestedSha": "d" * 40,
            "artifacts": {
                "runtime": {"repository": "ghcr.io/ronitnath/ronitnath-app", "digest": target},
                "migration": {"repository": "ghcr.io/ronitnath/ronitnath-app", "digest": "sha256:" + "e" * 64},
            },
        })
        try:
            with tempfile.TemporaryDirectory() as directory:
                coordinator.STATE = Path(directory)
                coordinator.LOCK = str(Path(directory) / "coordinator.lock")
                (coordinator.STATE / (release_id + ".manifest.json")).write_text(json.dumps(self.release))
                (coordinator.STATE / (release_id + ".state.json")).write_text(json.dumps({"state": "deploying", "step": "nyc-accepted", "migration": "complete"}))
                healthy = {"digest": target, "healthy": True}
                with patch.object(coordinator, "verify_candidate") as verify, patch.object(coordinator, "current_local", return_value=healthy), patch.object(coordinator, "remote", return_value=healthy), patch.object(coordinator, "run"), patch.object(coordinator, "ready"), patch.object(coordinator, "anonymous_denied"), patch.object(coordinator, "realtime_acceptance", return_value={"operator": True, "received": True, "applied": True}), patch.object(coordinator, "checkpoint", side_effect=lambda _release, state, step, **extra: {"state": state, "step": step, "migration": extra.get("migration", "complete"), "operations": {"sfo": "10000000-0000-4000-8000-000000000009"}}):
                    coordinator.main(release_id)
                verify.assert_not_called()
        finally:
            coordinator.STATE = original
            coordinator.LOCK = original_lock


class ReplicaRecoveryTests(unittest.TestCase):
    def test_repeated_operation_observes_completed_target_without_mutating(self):
        original = host_entry.STATE
        release_id = "10000000-0000-4000-8000-000000000003"
        operation_id = "10000000-0000-4000-8000-000000000004"
        expected = "sha256:" + "a" * 64
        target = "sha256:" + "b" * 64
        reference = "ghcr.io/ronitnath/ronitnath-app@" + target
        try:
            with tempfile.TemporaryDirectory() as directory:
                host_entry.STATE = Path(directory)
                observations = [
                    {"digest": expected, "healthy": True},
                    {"digest": target, "healthy": True},
                    {"digest": target, "healthy": True},
                ]
                with patch.object(host_entry, "inspect_container", side_effect=observations), patch.object(host_entry.subprocess, "run") as mutation:
                    first = host_entry.replica_mutation("replica-roll", release_id, operation_id, reference, expected, "c" * 40)
                    second = host_entry.replica_mutation("replica-roll", release_id, operation_id, reference, expected, "c" * 40)
                self.assertEqual(first["digest"], target)
                self.assertEqual(second["digest"], target)
                mutation.assert_called_once()
                record = json.loads((Path(directory) / (release_id + ".replica.json")).read_text())
                self.assertEqual(record["state"], "complete")
                self.assertEqual(record["operationId"], operation_id)
        finally:
            host_entry.STATE = original

    def test_conflicting_operation_is_rejected_before_mutation(self):
        original = host_entry.STATE
        release_id = "10000000-0000-4000-8000-000000000005"
        operation_id = "10000000-0000-4000-8000-000000000006"
        expected = "sha256:" + "a" * 64
        reference = "ghcr.io/ronitnath/ronitnath-app@sha256:" + "b" * 64
        try:
            with tempfile.TemporaryDirectory() as directory:
                host_entry.STATE = Path(directory)
                observations = [
                    {"digest": expected, "healthy": True},
                    {"digest": reference.rsplit("@", 1)[-1], "healthy": True},
                ]
                with patch.object(host_entry, "inspect_container", side_effect=observations), patch.object(host_entry.subprocess, "run") as mutation:
                    host_entry.replica_mutation("replica-roll", release_id, operation_id, reference, expected, "c" * 40)
                    with self.assertRaises(SystemExit):
                        host_entry.replica_mutation("replica-roll", release_id, "10000000-0000-4000-8000-000000000007", reference, expected, "c" * 40)
                mutation.assert_called_once()
        finally:
            host_entry.STATE = original


if __name__ == "__main__":
    unittest.main()
