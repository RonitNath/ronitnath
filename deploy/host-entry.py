#!/usr/bin/env python3
"""Forced-command boundary for status, submission, and replica operations."""

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

STATE = Path(os.environ.get("DELIVERY_STATE", "/data/crypt/ronitnath/releases-v2"))
LEGACY = Path(os.environ.get("DELIVERY_LEGACY_STATE", "/data/crypt/ronitnath/releases"))
DIGEST = re.compile(r"^sha256:[a-f0-9]{64}$")
IMAGE = re.compile(r"^ghcr\.io/ronitnath/ronitnath-app@sha256:[a-f0-9]{64}$")
SHA = re.compile(r"^[a-f0-9]{40}$")


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(64)


def inspect_container():
    try:
        reference = subprocess.run(["docker", "inspect", "ronitnath-web", "--format", "{{.Config.Image}}"], check=True, text=True, capture_output=True).stdout.strip()
        health = subprocess.run(["docker", "inspect", "ronitnath-web", "--format", "{{if .State.Health}}{{.State.Health.Status}}{{else}}unknown{{end}}"], check=True, text=True, capture_output=True).stdout.strip()
        environment = subprocess.run(["docker", "inspect", "ronitnath-web", "--format", "{{json .Config.Env}}"], check=True, text=True, capture_output=True).stdout
        version = next((item.split("=", 1)[1] for item in json.loads(environment) if item.startswith("APP_VERSION=")), None)
        size = int(subprocess.run(["docker", "image", "inspect", reference, "--format", "{{.Size}}"], check=True, text=True, capture_output=True).stdout.strip())
        return {"reference": reference, "digest": reference.rsplit("@", 1)[-1] if "@" in reference else None, "healthy": health == "healthy", "sourceSha": version, "sizeBytes": size}
    except (subprocess.CalledProcessError, ValueError, json.JSONDecodeError):
        return {"reference": None, "digest": None, "healthy": False, "sourceSha": None, "sizeBytes": None}


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".")
    try:
        with os.fdopen(fd, "w") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def replica_mutation(action, release_id, operation_id, reference, expected, source_sha):
    target = reference.rsplit("@", 1)[-1]
    operation_path = STATE / (release_id + ".replica.json")
    intent = {
        "schemaVersion": 2,
        "releaseId": release_id,
        "operationId": operation_id,
        "action": action,
        "reference": reference,
        "expectedDigest": expected,
        "targetDigest": target,
        "sourceSha": source_sha,
    }
    if operation_path.exists():
        recorded = json.loads(operation_path.read_text())
        comparable = {key: recorded.get(key) for key in intent}
        if comparable != intent:
            rollback_of_completed_roll = (
                action == "replica-rollback"
                and recorded.get("action") == "replica-roll"
                and recorded.get("state") == "complete"
                and recorded.get("targetDigest") == expected
            )
            if not rollback_of_completed_roll:
                fail("replica release already has a different operation")
            atomic(operation_path, json.dumps({**intent, "state": "intent"}, separators=(",", ":")))
    else:
        atomic(operation_path, json.dumps({**intent, "state": "intent"}, separators=(",", ":")))
    observed = inspect_container()
    if observed["digest"] == target and observed["healthy"]:
        atomic(operation_path, json.dumps({**intent, "state": "complete", "observed": observed}, separators=(",", ":")))
        return observed
    if observed["digest"] != expected:
        fail("replica compare-and-swap failed")
    subprocess.run(["env", "IMAGE=" + reference, "APP_VERSION=" + source_sha, "docker", "compose", "-f", "/data/crypt/ronitnath/compose.yaml", "up", "-d", "--wait", "--wait-timeout", "120"], check=True)
    observed = inspect_container()
    if observed["digest"] != target or not observed["healthy"]:
        fail("replica did not reach the requested healthy digest")
    atomic(operation_path, json.dumps({**intent, "state": "complete", "observed": observed}, separators=(",", ":")))
    return observed


def valid_id(value):
    try:
        return str(uuid.UUID(value)) == value
    except ValueError:
        return False


def exact_keys(value, keys, name):
    if not isinstance(value, dict) or set(value) != set(keys):
        fail(name + " contains missing or unknown fields")


def validate_artifact(value, name):
    exact_keys(value, {"repository", "tag", "digest", "sourceSha", "platform", "sizeBytes"}, name)
    if value["repository"] != "ghcr.io/ronitnath/ronitnath-app" or not DIGEST.fullmatch(value["digest"]) or not SHA.fullmatch(value["sourceSha"]):
        fail(name + " is invalid")
    if value["platform"] != "linux/amd64" or (value["sizeBytes"] is not None and (not isinstance(value["sizeBytes"], int) or value["sizeBytes"] < 0)):
        fail(name + " has invalid platform or size")


def validate_manifest(value, release_id, claimed):
    exact_keys(value, {"schemaVersion", "releaseId", "requestedSha", "createdAt", "artifacts", "migrations", "compatibility", "previousRuntimeDigest", "manifestDigest"}, "manifest")
    if value["schemaVersion"] != 2 or value["releaseId"] != release_id or not SHA.fullmatch(value["requestedSha"]) or not DIGEST.fullmatch(value["previousRuntimeDigest"]):
        fail("manifest identity is invalid")
    exact_keys(value["artifacts"], {"runtime", "migration"}, "artifacts")
    validate_artifact(value["artifacts"]["runtime"], "runtime artifact")
    validate_artifact(value["artifacts"]["migration"], "migration artifact")
    if value["artifacts"]["runtime"]["sourceSha"] != value["requestedSha"] or value["artifacts"]["migration"]["sourceSha"] != value["requestedSha"]:
        fail("artifact source does not match requested SHA")
    exact_keys(value["migrations"], {"schemaVersion", "entries"}, "migrations")
    if value["migrations"]["schemaVersion"] != 2 or not isinstance(value["migrations"]["entries"], list):
        fail("migration policy is invalid")
    for entry in value["migrations"]["entries"]:
        exact_keys(entry, {"name", "checksum", "mode", "transactional", "lockTimeoutMs", "statementTimeoutMs", "compatibleRuntimeDigests", "backfill"}, "migration")
        if not re.fullmatch(r"[a-f0-9]{64}", entry["checksum"]) or entry["mode"] not in ("expand", "transition", "contract") or not isinstance(entry["transactional"], bool):
            fail("migration declaration is invalid")
        if value["previousRuntimeDigest"] not in entry["compatibleRuntimeDigests"]:
            fail("migration omits the previous runtime digest")
    compatibility = value["compatibility"]
    required = {"previousRuntime", "previousMigration", "previousSchemaChecksums", "candidateSchemaChecksums", "previousRead", "previousWrite", "candidateRead", "candidateWrite", "rollbackRead", "repeatedMigration", "testedAt"}
    exact_keys(compatibility, required, "compatibility evidence")
    validate_artifact(compatibility["previousRuntime"], "previous runtime")
    if compatibility["previousMigration"] is not None:
        validate_artifact(compatibility["previousMigration"], "previous migration")
    if compatibility["previousRuntime"]["digest"] != value["previousRuntimeDigest"] or not all(compatibility[key] is True for key in ("previousRead", "previousWrite", "candidateRead", "candidateWrite", "rollbackRead", "repeatedMigration")):
        fail("compatibility evidence is incomplete")
    without_digest = dict(value)
    without_digest.pop("manifestDigest")
    calculated = "sha256:" + hashlib.sha256(json.dumps(without_digest, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    if value["manifestDigest"] != claimed or calculated != claimed:
        fail("manifest binding failed")


def legacy_record(digest):
    for path in sorted(LEGACY.glob("*.json"), key=lambda item: item.stat().st_mtime, reverse=True):
        try:
            value = json.loads(path.read_text())
            if str(value.get("runtime", "")).endswith("@" + digest):
                return value
        except (OSError, json.JSONDecodeError):
            continue
    return None


def deployed_manifest(digest):
    for state_path in sorted(STATE.glob("*.state.json"), key=lambda item: item.stat().st_mtime, reverse=True):
        try:
            state = json.loads(state_path.read_text())
            manifest = json.loads((STATE / (state["releaseId"] + ".manifest.json")).read_text())
            if state.get("state") == "deployed" and manifest["artifacts"]["runtime"]["digest"] == digest:
                return manifest
        except (KeyError, OSError, json.JSONDecodeError):
            continue
    return None


def baseline():
    local = inspect_container()
    peer = json.loads(subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", "-o", "UserKnownHostsFile=/etc/ronitnath-delivery/sfo-known-hosts", "-i", "/etc/ronitnath-delivery/sfo-key", "delivery-replica@sfo", "replica-status", "current"], check=True, text=True, capture_output=True).stdout)
    agreed = bool(local["digest"] and local["digest"] == peer.get("digest"))
    manifest = deployed_manifest(local["digest"]) if local["digest"] else None
    if manifest:
        return {"schemaVersion": 2, "agreed": agreed, "runtime": manifest["artifacts"]["runtime"], "migration": manifest["artifacts"]["migration"], "schemaChecksums": manifest["compatibility"]["candidateSchemaChecksums"], "nyc": local, "sfo": peer}
    record = legacy_record(local["digest"]) if local["digest"] else None
    if not agreed or not record or not local["sourceSha"] or not SHA.fullmatch(local["sourceSha"]):
        return {"schemaVersion": 2, "agreed": False, "nyc": local, "sfo": peer}
    migration_reference = record.get("migrate")
    if not isinstance(migration_reference, str) or "@" not in migration_reference:
        return {"schemaVersion": 2, "agreed": False, "nyc": local, "sfo": peer}
    runtime = {"repository": "ghcr.io/ronitnath/ronitnath-app", "tag": local["reference"], "digest": local["digest"], "sourceSha": local["sourceSha"], "platform": "linux/amd64", "sizeBytes": local["sizeBytes"]}
    migration = {"repository": "ghcr.io/ronitnath/ronitnath-app", "tag": migration_reference, "digest": migration_reference.rsplit("@", 1)[-1], "sourceSha": local["sourceSha"], "platform": "linux/amd64", "sizeBytes": None}
    return {"schemaVersion": 2, "agreed": True, "runtime": runtime, "migration": migration, "schemaChecksums": {}, "nyc": local, "sfo": peer}


def command():
    if len(sys.argv) != 3 or sys.argv[1] not in ("status", "submit", "replica"):
        fail("invalid forced-command role")
    role = sys.argv[1]
    parts = sys.argv[2].split()
    action = parts[0] if parts else ""
    allowed = {
        "status": {"status"},
        "submit": {"submit", "reconcile"},
        "replica": {"replica-status", "replica-preflight", "replica-roll", "replica-rollback"},
    }
    if action not in allowed[role]:
        fail("command is outside this service account role")
    if action in ("status", "replica-status"):
        release = parts[1] if len(parts) > 1 else "current"
        if release == "current" and action == "status":
            print(json.dumps(baseline(), separators=(",", ":")))
            return
        path = STATE / (release + ".state.json")
        print(path.read_text() if path.exists() else json.dumps({"releaseId": release, **inspect_container()}, separators=(",", ":")))
        return
    if action == "submit":
        if len(parts) != 3 or not valid_id(parts[1]) or not DIGEST.fullmatch(parts[2]):
            fail("invalid submission identity")
        body = sys.stdin.buffer.read(1024 * 1024 + 1)
        if len(body) > 1024 * 1024:
            fail("manifest too large")
        try:
            value = json.loads(body)
        except json.JSONDecodeError:
            fail("manifest is not JSON")
        validate_manifest(value, parts[1], parts[2])
        path = STATE / (parts[1] + ".manifest.json")
        encoded = json.dumps(value, separators=(",", ":"))
        if path.exists() and json.loads(path.read_text()) != value:
            fail("release identity already has different content")
        if not path.exists():
            atomic(path, encoded)
        subprocess.run(["systemctl", "enable", "--now", "--no-block", "ronitnath-delivery@" + parts[1] + ".service"], check=True)
        print(json.dumps({"accepted": True, "releaseId": parts[1]}))
        return
    if action == "reconcile":
        if len(parts) != 2 or not valid_id(parts[1]):
            fail("invalid release identity")
        subprocess.run(["systemctl", "start", "--no-block", "ronitnath-delivery@" + parts[1] + ".service"], check=True)
        print(json.dumps({"started": True, "releaseId": parts[1]}))
        return
    if action == "replica-preflight":
        if len(parts) != 4 or not valid_id(parts[1]) or not IMAGE.fullmatch(parts[2]) or not DIGEST.fullmatch(parts[3]):
            fail("invalid replica preflight")
        observed = inspect_container()
        if observed["digest"] != parts[3]:
            fail("replica baseline mismatch")
        subprocess.run(["docker", "pull", parts[2]], check=True)
        print(json.dumps(observed))
        return
    if action in ("replica-roll", "replica-rollback"):
        if len(parts) != 6 or not valid_id(parts[1]) or not valid_id(parts[2]) or not IMAGE.fullmatch(parts[3]) or not DIGEST.fullmatch(parts[4]) or not SHA.fullmatch(parts[5]):
            fail("invalid replica mutation")
        print(json.dumps(replica_mutation(action, parts[1], parts[2], parts[3], parts[4], parts[5])))
        return
    fail("unsupported deployment command")


if __name__ == "__main__":
    command()
