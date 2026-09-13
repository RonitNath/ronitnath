#!/usr/bin/env python3
"""Durable, single-owner production coordinator for ronitnath.com."""

import fcntl
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

STATE = Path(os.environ.get("DELIVERY_STATE", "/data/crypt/ronitnath/releases-v2"))
LOCK = os.environ.get("DELIVERY_LOCK", "/run/lock/ronitnath-delivery.lock")
COMPOSE = os.environ.get("DELIVERY_COMPOSE", "/data/crypt/ronitnath/compose.yaml")
ENV = os.environ.get("DELIVERY_ENV", "/data/crypt/ronitnath/web.env")
SFO_SSH = os.environ.get("DELIVERY_SFO_SSH", "delivery-replica@sfo")
TELEMETRY_URL = os.environ.get("DELIVERY_TELEMETRY_URL", "https://ronitnath.com/api/delivery/events")
TELEMETRY_TOKEN_FILE = Path(os.environ.get("DELIVERY_TELEMETRY_TOKEN_FILE", "/etc/ronitnath-delivery/telemetry-token"))
TELEMETRY_LIMIT = 10 * 1024 * 1024
ACCEPTANCE_TOKEN_FILE = Path(os.environ.get("DELIVERY_ACCEPTANCE_TOKEN_FILE", "/etc/ronitnath-delivery/acceptance-token"))
SFO_HTTP = os.environ.get("DELIVERY_SFO_HTTP", "http://100.88.42.226:3140")


class TransientUnavailable(RuntimeError):
    pass


def run(args, *, input_text=None):
    return subprocess.run(args, input=input_text, text=True, check=True, capture_output=True).stdout.strip()


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".")
    try:
        with os.fdopen(fd, "w") as output:
            if isinstance(data, str):
                output.write(data)
            else:
                json.dump(data, output, separators=(",", ":"), sort_keys=True)
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


def append_line(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a") as output:
        output.write(json.dumps(value, separators=(",", ":")) + "\n")
        output.flush()
        os.fsync(output.fileno())


def sequence(release_id):
    path = STATE / (release_id + ".journal.jsonl")
    if not path.exists():
        return 0
    maximum = -1
    for line in path.read_text().splitlines():
        try:
            maximum = max(maximum, int(json.loads(line)["seq"]))
        except (KeyError, ValueError, json.JSONDecodeError):
            continue
    return maximum + 1


def post_events(events):
    token = TELEMETRY_TOKEN_FILE.read_text().strip()
    request = urllib.request.Request(TELEMETRY_URL, data=json.dumps(events).encode(), method="POST", headers={"Authorization": "Bearer " + token, "Content-Type": "application/json", "User-Agent": "fleet-delivery/2"})
    with urllib.request.urlopen(request, timeout=2) as response:
        if response.status != 200:
            raise RuntimeError("telemetry returned " + str(response.status))


def buffer_event(release_id, event):
    path = STATE / (release_id + ".telemetry.jsonl")
    lines = path.read_text().splitlines() if path.exists() else []
    lines.append(json.dumps(event, separators=(",", ":")))
    dropped = []
    while len(("\n".join(lines) + "\n").encode()) > TELEMETRY_LIMIT and len(lines) > 1:
        removed = json.loads(lines.pop(0))
        if removed.get("type") == "telemetry-gap":
            dropped.extend([removed["firstDroppedSeq"], removed["lastDroppedSeq"]])
        else:
            dropped.append(removed["seq"])
    if dropped:
        gap = {"schemaVersion": 2, "releaseId": release_id, "producerId": "nyc-coordinator:buffer", "seq": event["seq"], "at": event["at"], "type": "telemetry-gap", "firstDroppedSeq": min(dropped), "lastDroppedSeq": max(dropped)}
        lines.insert(0, json.dumps(gap, separators=(",", ":")))
        while len(("\n".join(lines) + "\n").encode()) > TELEMETRY_LIMIT and len(lines) > 2:
            removed = json.loads(lines.pop(1))
            gap["firstDroppedSeq"] = min(gap["firstDroppedSeq"], removed.get("firstDroppedSeq", removed["seq"]))
            gap["lastDroppedSeq"] = max(gap["lastDroppedSeq"], removed.get("lastDroppedSeq", removed["seq"]))
            lines[0] = json.dumps(gap, separators=(",", ":"))
    atomic(path, "\n".join(lines) + "\n")


def publish_event(release_id, event):
    buffer = STATE / (release_id + ".telemetry.jsonl")
    try:
        post_events([event])
        if buffer.exists() and buffer.stat().st_size:
            pending = [json.loads(line) for line in buffer.read_text().splitlines()]
            for offset in range(0, len(pending), 100):
                post_events(pending[offset : offset + 100])
            atomic(buffer, "")
    except (OSError, ValueError, urllib.error.URLError, RuntimeError):
        buffer_event(release_id, event)


def current_local():
    reference = run(["docker", "inspect", "ronitnath-web", "--format", "{{.Config.Image}}"])
    health = run(["docker", "inspect", "ronitnath-web", "--format", "{{if .State.Health}}{{.State.Health.Status}}{{else}}unknown{{end}}"])
    return {"reference": reference, "digest": reference.rsplit("@", 1)[-1] if "@" in reference else None, "healthy": health == "healthy"}


def remote(*args):
    command = ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", "-o", "UserKnownHostsFile=/etc/ronitnath-delivery/sfo-known-hosts", "-i", "/etc/ronitnath-delivery/sfo-key", SFO_SSH, *args]
    try:
        return json.loads(run(command))
    except subprocess.CalledProcessError as error:
        if error.returncode == 255:
            raise TransientUnavailable("SFO SSH is unavailable") from error
        raise


def checkpoint(release, state, step, **extra):
    at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    operations = {name: str(uuid.uuid5(uuid.UUID(release["releaseId"]), name + "-roll")) for name in ("nyc", "sfo")}
    value = {"schemaVersion": 2, "releaseId": release["releaseId"], "state": state, "step": step, "updatedAt": at, "operations": operations, **extra}
    atomic(STATE / (release["releaseId"] + ".state.json"), value)
    replicas = {}
    for name in ("nyc", "sfo"):
        observed = extra.get(name)
        if isinstance(observed, dict):
            replicas[name] = {"digest": observed.get("digest"), "healthy": bool(observed.get("healthy"))}
    seq = sequence(release["releaseId"])
    event = {
        "schemaVersion": 2,
        "releaseId": release["releaseId"],
        "producerId": "nyc-coordinator",
        "seq": seq,
        "at": at,
        "type": "coordinator-checkpoint",
        "state": state,
        "step": step,
        "migration": extra.get("migration", "pending"),
        "replicas": replicas,
        "error": str(extra["error"]) if "error" in extra else None,
        "manifest": release if seq == 0 else None,
    }
    append_line(STATE / (release["releaseId"] + ".journal.jsonl"), event)
    publish_event(release["releaseId"], event)
    return value


def get_json(url):
    request = urllib.request.Request(url, headers={"User-Agent": "fleet-delivery/2"})
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError:
        raise
    except urllib.error.URLError as error:
        raise TransientUnavailable(url + " is unavailable") from error


def verify_candidate(sha):
    status, deploy = get_json("https://api.github.com/repos/RonitNath/ronitnath/git/ref/heads/deploy")
    if status != 200 or deploy.get("object", {}).get("sha") != sha:
        raise RuntimeError("deploy branch moved before production ownership")
    status, comparison = get_json(f"https://api.github.com/repos/RonitNath/ronitnath/compare/{sha}...main")
    if status != 200 or comparison.get("status") not in ("ahead", "identical"):
        raise RuntimeError("candidate is no longer reachable from main")


def ready(url, sha):
    status, body = get_json(url)
    if status != 200 or not body.get("ok") or body.get("version") != sha:
        raise RuntimeError("acceptance mismatch at " + url)


def anonymous_denied(url):
    try:
        urllib.request.urlopen(url, timeout=15)
    except urllib.error.HTTPError as error:
        if error.code in (401, 403, 404):
            return
        raise
    raise RuntimeError("anonymous inspector unexpectedly accessible")


def post_json(url, value, token=None):
    headers = {"Content-Type": "application/json", "User-Agent": "fleet-delivery/2"}
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(url, data=json.dumps(value).encode(), method="POST", headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = response.read()
            return response.status, json.loads(body) if body else None
    except urllib.error.HTTPError:
        raise
    except urllib.error.URLError as error:
        raise TransientUnavailable(url + " is unavailable") from error


def realtime_acceptance(release):
    token = ACCEPTANCE_TOKEN_FILE.read_text().strip()
    visitor_id, tab_id, view_id, fixture_id = (str(uuid.uuid4()) for _ in range(4))
    registration = {"action": "register", "view": {"visitorId": visitor_id, "tabId": tab_id, "viewId": view_id, "path": "/", "title": "Delivery acceptance", "subscriptions": [{"type": "configuration", "key": "homepage", "state": "published"}], "viewport": {"visible": True, "sectionIds": ["delivery-acceptance"], "rowIds": []}, "interactedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}}
    post_json("http://127.0.0.1:3140/api/realtime", registration)
    try:
        _, started = post_json("http://127.0.0.1:3140/api/delivery/acceptance", {"action": "start", "releaseId": release["releaseId"], "fixtureId": fixture_id}, token)
        query = urllib.parse.urlencode({"visitor": visitor_id, "view": view_id})
        delivery = None
        try:
            with urllib.request.urlopen(SFO_HTTP + "/api/realtime/stream?" + query, timeout=20) as response:
                for _ in range(200):
                    line = response.readline().decode().strip()
                    if line.startswith("data: "):
                        candidate = json.loads(line[6:])
                        if candidate.get("eventSeq") == started["eventSeq"]:
                            delivery = candidate
                            break
        except urllib.error.HTTPError:
            raise
        except urllib.error.URLError as error:
            raise TransientUnavailable("SFO realtime stream is unavailable") from error
        if not delivery:
            raise RuntimeError("SFO did not return the cross-replica delivery")
        for stage in ("received", "applied"):
            post_json(SFO_HTTP + "/api/realtime", {"action": "acknowledge", "visitorId": visitor_id, "viewId": view_id, "acknowledgment": {"deliveryId": delivery["id"], "revision": delivery["revision"], "stage": stage}})
        _, result = post_json("http://127.0.0.1:3140/api/delivery/acceptance", {"action": "finish", "fixtureId": fixture_id, "viewId": view_id, "eventSeq": started["eventSeq"]}, token)
        return result
    finally:
        try:
            post_json("http://127.0.0.1:3140/api/delivery/acceptance", {"action": "cleanup", "fixtureId": fixture_id}, token)
            post_json(SFO_HTTP + "/api/realtime", {"action": "disconnect", "visitorId": visitor_id, "viewId": view_id})
        except Exception:
            pass


def migrate(release, image, state):
    if state.get("migration") == "complete":
        return state
    if state.get("migration") == "intent":
        probe = subprocess.run(["docker", "run", "--rm", "--network", "host", "--env-file", ENV, "-e", "MIGRATION_PROBE_ONLY=1", image], text=True, capture_output=True)
        if probe.returncode == 0:
            return checkpoint(release, "deploying", "migration-reconciled", migration="complete")
        if not all(entry["transactional"] for entry in release["migrations"]["entries"]):
            return checkpoint(release, "degraded", "migration-unknown", migration="unknown", error=probe.stderr[-2000:])
    state = checkpoint(release, "deploying", "migration-intent", migration="intent")
    run(["docker", "run", "--rm", "--network", "host", "--env-file", ENV, image])
    return checkpoint(release, "deploying", "migration-complete", migration="complete")


def roll_local(reference, sha):
    run(["env", "IMAGE=" + reference, "APP_VERSION=" + sha, "docker", "compose", "-f", COMPOSE, "up", "-d", "--wait", "--wait-timeout", "120"])


def rollback_changed(release, target, state, error):
    previous = release["previousRuntimeDigest"]
    artifact = release["compatibility"]["previousRuntime"]
    reference = artifact["repository"] + "@" + previous
    compatible = release["compatibility"]["rollbackRead"] and state.get("migration") != "unknown"
    if not compatible:
        checkpoint(release, "degraded", "stopped", error=str(error), migration=state.get("migration"))
        return
    failures = []
    changed = False
    try:
        if current_local().get("digest") == target:
            changed = True
            roll_local(reference, artifact["sourceSha"])
    except Exception as rollback_error:
        failures.append("nyc: " + str(rollback_error))
    try:
        peer = remote("replica-status", release["releaseId"])
        if peer.get("digest") == target:
            changed = True
            operation_id = str(uuid.uuid5(uuid.UUID(release["releaseId"]), "sfo-rollback"))
            remote("replica-rollback", release["releaseId"], operation_id, reference, target, artifact["sourceSha"])
    except Exception as rollback_error:
        failures.append("sfo: " + str(rollback_error))
    if failures:
        checkpoint(release, "degraded", "rollback-failed", error=str(error), rollbackError="; ".join(failures), migration=state.get("migration"))
    elif not changed:
        checkpoint(release, "failed", "failed-before-mutation", error=str(error), migration=state.get("migration"))
    else:
        checkpoint(release, "failed", "rolled-back", error=str(error), migration=state.get("migration"))


def main(release_id):
    release = json.loads((STATE / (release_id + ".manifest.json")).read_text())
    target = release["artifacts"]["runtime"]["digest"]
    previous = release["previousRuntimeDigest"]
    runtime = release["artifacts"]["runtime"]["repository"] + "@" + target
    migration = release["artifacts"]["migration"]["repository"] + "@" + release["artifacts"]["migration"]["digest"]
    with open(LOCK, "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        state_path = STATE / (release_id + ".state.json")
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        if state.get("state") in ("deployed", "failed", "degraded"):
            return
        try:
            if not state:
                verify_candidate(release["requestedSha"])
            local = current_local()
            peer = remote("replica-status", release_id)
            allowed = {previous, target}
            if local["digest"] not in allowed or peer.get("digest") not in allowed:
                raise RuntimeError("production baseline changed outside this operation")
            state = checkpoint(release, "deploying", "observed", migration=state.get("migration", "pending"), nyc=local, sfo=peer)
            run(["docker", "pull", runtime])
            remote("replica-preflight", release_id, runtime, previous)
            state = migrate(release, migration, state)
            if state.get("migration") == "unknown":
                raise RuntimeError("migration outcome is unknown")
            if current_local()["digest"] != target:
                state = checkpoint(release, "deploying", "nyc-roll-intent", migration="complete")
                roll_local(runtime, release["requestedSha"])
            ready("http://127.0.0.1:3140/readyz", release["requestedSha"])
            state = checkpoint(release, "deploying", "nyc-accepted", migration="complete")
            peer = remote("replica-status", release_id)
            if peer.get("digest") != target:
                state = checkpoint(release, "deploying", "sfo-roll-intent", migration="complete", nyc=current_local(), sfo=peer)
                remote("replica-roll", release_id, state["operations"]["sfo"], runtime, previous, release["requestedSha"])
            peer = remote("replica-status", release_id)
            if peer.get("digest") != target or not peer.get("healthy"):
                raise RuntimeError("SFO direct acceptance failed")
            state = checkpoint(release, "deploying", "sfo-accepted", migration="complete")
            ready("https://ronitnath.com/readyz", release["requestedSha"])
            anonymous_denied("https://ronitnath.com/o/isoastra/delivery")
            state = checkpoint(release, "deploying", "public-acceptance-intent", migration="complete", nyc=current_local(), sfo=peer)
            live = realtime_acceptance(release)
            checkpoint(release, "deployed", "finished", migration="complete", acceptance={"replicas": True, "public": True, "anonymousDenied": True, "operator": live["operator"], "received": live["received"], "applied": live["applied"]}, nyc=current_local(), sfo=remote("replica-status", release_id))
        except TransientUnavailable as error:
            checkpoint(release, "deploying", "waiting-for-recovery", migration=state.get("migration", "pending"), error=str(error))
            raise
        except Exception as error:
            rollback_changed(release, target, state, error)
            raise


if __name__ == "__main__":
    main(sys.argv[1])
