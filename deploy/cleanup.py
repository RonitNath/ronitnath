#!/usr/bin/env python3
"""Compact completed release evidence and remove expired detail safely."""

import json
import os
import tempfile
import time
from pathlib import Path

STATE = Path(os.environ.get("DELIVERY_STATE", "/data/crypt/ronitnath/releases-v2"))
DETAIL_SECONDS = 30 * 24 * 60 * 60
SUMMARY_SECONDS = 365 * 24 * 60 * 60


def atomic(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".")
    try:
        with os.fdopen(fd, "w") as output:
            json.dump(value, output, separators=(",", ":"), sort_keys=True)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def compact(now=None):
    now = now or time.time()
    STATE.mkdir(parents=True, exist_ok=True)
    for state_path in STATE.glob("*.state.json"):
        release_id = state_path.name.removesuffix(".state.json")
        if now - state_path.stat().st_mtime <= DETAIL_SECONDS:
            continue
        state = json.loads(state_path.read_text())
        if state.get("state") not in ("deployed", "failed", "degraded"):
            continue
        manifest_path = STATE / (release_id + ".manifest.json")
        manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else None
        summary_path = STATE / (release_id + ".summary.json")
        if not summary_path.exists():
            atomic(summary_path, {
                "schemaVersion": 2,
                "releaseId": release_id,
                "state": state.get("state"),
                "step": state.get("step"),
                "updatedAt": state.get("updatedAt"),
                "requestedSha": manifest.get("requestedSha") if manifest else None,
                "manifestDigest": manifest.get("manifestDigest") if manifest else None,
                "runtimeDigest": manifest.get("artifacts", {}).get("runtime", {}).get("digest") if manifest else None,
            })
        for suffix in (".journal.jsonl", ".telemetry.jsonl", ".manifest.json", ".state.json"):
            path = STATE / (release_id + suffix)
            if path.exists():
                path.unlink()
    for summary in STATE.glob("*.summary.json"):
        if now - summary.stat().st_mtime > SUMMARY_SECONDS:
            summary.unlink()


if __name__ == "__main__":
    compact()
