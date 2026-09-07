#!/usr/bin/env python3
"""Build the streamed Gaia DR3 LOD assets for the sky (plan S2).

A 32x24 equal-angle sky grid (768 addressable tiles). Each tile is its own
static file `public/stars/lod/<id>.bin` so the client can fetch the tiles it
needs, view-first, with immutable cache headers and no server logic. `g9.bin`
carries every G <= 9 record in one file for the after-first-paint fetch.

Record: "<Qhhhh" = source_id, octahedral x, octahedral y, mag*1000, (BP-RP)*1000.
File:   b"GDR3LOD1" + u32 record count + records.

Records whose source_id is already in the bright catalog
(`tools/starcat/data/gaia_g6.5.csv`, shipped as STR2) are dropped here; the
count lands in the manifest as `droppedBright`.

Two sources, in order of preference:
  --from-legacy DIR  slice the already-built legacy blobs (g12.bin + its
                     manifest); no network, byte-identical to the pinned pull.
  --from-tap         re-query the Gaia archive per tile (slow; only when the
                     pinned release is refreshed).

Python 3 standard library only. Production never contacts ESA.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import math
import os
import struct
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OUT = ROOT / "public" / "stars" / "lod"
TAP = "https://gea.esac.esa.int/tap-server/tap/sync"
QUERY = """SELECT source_id,ra,dec,phot_g_mean_mag,bp_rp
FROM gaiadr3.gaia_source
WHERE phot_g_mean_mag <= 12
  AND ra >= {ra_lo} AND ra < {ra_hi}
  AND dec >= {dec_lo} AND dec < {dec_hi}"""
MAGIC = b"GDR3LOD1"
RECORD = struct.Struct("<Qhhhh")
HEADER = struct.Struct("<I")
RA_BINS, DEC_BINS = 32, 24
G_LIMIT, G9_LIMIT = 12, 9


def oct_encode(ra_deg: float, dec_deg: float) -> tuple[int, int]:
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    x, y, z = math.cos(dec) * math.cos(ra), math.cos(dec) * math.sin(ra), math.sin(dec)
    scale = abs(x) + abs(y) + abs(z)
    x, y, z = x / scale, y / scale, z / scale
    if z < 0:
        x, y = (1 - abs(y)) * (1 if x >= 0 else -1), (1 - abs(x)) * (1 if y >= 0 else -1)
    return round(x * 32767), round(y * 32767)


def tile_for(ra: float, dec: float) -> int:
    col = min(RA_BINS - 1, int((ra % 360) / 360 * RA_BINS))
    row = min(DEC_BINS - 1, max(0, int((dec + 90) / 180 * DEC_BINS)))
    return row * RA_BINS + col


def bright_source_ids() -> set[int]:
    path = Path(os.environ.get("RN_BRIGHT_DIR", HERE / "data")) / "gaia_g6.5.csv"
    if not path.exists():
        raise SystemExit(f"bright catalog not found at {path} "
                         "(set RN_BRIGHT_DIR to the directory holding gaia_g6.5.csv)")
    with path.open(newline="", encoding="utf-8") as handle:
        return {int(row["source_id"]) for row in csv.DictReader(handle)}


def sha256_of(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# --------------------------------------------------------------- legacy slice

def buckets_from_legacy(directory: Path) -> tuple[list[bytearray], dict]:
    """Slice the pinned g12.bin into per-tile record buffers."""
    manifest = json.loads((directory / "manifest.json").read_text())
    blob = (directory / "g12.bin").read_bytes()
    if blob[:8] != MAGIC:
        raise SystemExit("legacy g12.bin has an unexpected magic")
    total = HEADER.unpack_from(blob, 8)[0]
    if total != sum(tile["count"] for tile in manifest["tiles"]):
        raise SystemExit("legacy g12.bin header count disagrees with its manifest")
    if manifest["recordBytes"] != RECORD.size or \
            (manifest["raBins"], manifest["decBins"]) != (RA_BINS, DEC_BINS):
        raise SystemExit("legacy manifest grid does not match this builder")
    buckets: list[bytearray] = []
    for tile in manifest["tiles"]:
        chunk = blob[tile["offset"]:tile["offset"] + tile["bytes"]]
        if len(chunk) != tile["bytes"] or tile["bytes"] != tile["count"] * RECORD.size:
            raise SystemExit(f"legacy tile {tile['id']} is short")
        buckets.append(bytearray(chunk))
    print(f"legacy: {total:,} G<={G_LIMIT} records across {len(buckets)} tiles")
    return buckets, manifest


# ------------------------------------------------------------------ TAP pull

def download_tile(job) -> Path:
    path, tile, ra_lo, ra_hi, dec_lo, dec_hi = job
    if path.exists() and path.stat().st_size > 30:
        return path
    query = QUERY.format(ra_lo=ra_lo, ra_hi=ra_hi, dec_lo=dec_lo, dec_hi=dec_hi)
    payload = urllib.parse.urlencode(
        {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv", "QUERY": query}).encode()
    for attempt in range(5):
        try:
            with urllib.request.urlopen(urllib.request.Request(TAP, data=payload),
                                        timeout=300) as response:
                body = response.read()
            path.with_suffix(".part").write_bytes(body)
            path.with_suffix(".part").replace(path)
            print(f"downloaded tile {tile:03}", flush=True)
            return path
        except Exception:  # noqa: BLE001
            if attempt == 4:
                raise
            time.sleep(2 ** attempt)
    raise RuntimeError("unreachable")


def buckets_from_tap(cache: Path) -> tuple[list[bytearray], dict]:
    cache.mkdir(parents=True, exist_ok=True)
    jobs = []
    for tile in range(RA_BINS * DEC_BINS):
        row, col = divmod(tile, RA_BINS)
        jobs.append((cache / f"gaia-{tile:03}.csv", tile,
                     col * 360 / RA_BINS, (col + 1) * 360 / RA_BINS,
                     row * 180 / DEC_BINS - 90, (row + 1) * 180 / DEC_BINS - 90))
    with ThreadPoolExecutor(max_workers=4) as pool:
        sources = list(pool.map(download_tile, jobs))
    buckets = [bytearray() for _ in range(RA_BINS * DEC_BINS)]
    count = 0
    for source in sources:
        with source.open("rb") as raw:
            for row in csv.DictReader(io.TextIOWrapper(raw, encoding="utf-8", newline="")):
                colour = 0 if not row["bp_rp"] else \
                    round(max(-3.0, min(6.0, float(row["bp_rp"]))) * 1000)
                ra, dec = float(row["ra"]), float(row["dec"])
                mag = float(row["phot_g_mean_mag"])
                ox, oy = oct_encode(ra, dec)
                buckets[tile_for(ra, dec)].extend(
                    RECORD.pack(int(row["source_id"]), ox, oy, round(mag * 1000), colour))
                count += 1
    print(f"tap: {count:,} G<={G_LIMIT} records")
    return buckets, {"release": "Gaia DR3"}


# ------------------------------------------------------------------- writing

def build(buckets: list[bytearray], release: str) -> dict:
    bright = bright_source_ids()
    print(f"bright catalog: {len(bright):,} source ids to drop")
    OUT.mkdir(parents=True, exist_ok=True)
    for stale in OUT.glob("*.bin"):
        stale.unlink()

    manifest = {
        "version": 2,
        "release": release,
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "magic": MAGIC.decode(),
        "recordBytes": RECORD.size,
        "raBins": RA_BINS,
        "decBins": DEC_BINS,
        "gLimit": G_LIMIT,
        "g9Limit": G9_LIMIT,
        "path": "/stars/lod",
        "tiles": [],
    }
    mid = bytearray()
    kept = dropped = 0
    for tile, data in enumerate(buckets):
        body = bytearray()
        for offset in range(0, len(data), RECORD.size):
            record = data[offset:offset + RECORD.size]
            source_id, _ox, _oy, milli_mag, _colour = RECORD.unpack(record)
            if source_id in bright:
                dropped += 1
                continue
            body.extend(record)
            if milli_mag <= G9_LIMIT * 1000:
                mid.extend(record)
            kept += 1
        blob = MAGIC + HEADER.pack(len(body) // RECORD.size) + bytes(body)
        (OUT / f"{tile}.bin").write_bytes(blob)
        manifest["tiles"].append({
            "id": tile,
            "file": f"{tile}.bin",
            "count": len(body) // RECORD.size,
            "bytes": len(blob),
            "sha256": sha256_of(blob),
        })

    g9 = MAGIC + HEADER.pack(len(mid) // RECORD.size) + bytes(mid)
    (OUT / "g9.bin").write_bytes(g9)
    manifest["g9"] = {"file": "g9.bin", "count": len(mid) // RECORD.size,
                      "bytes": len(g9), "sha256": sha256_of(g9)}
    manifest["count"] = kept
    manifest["droppedBright"] = dropped
    manifest["totalBytes"] = sum(t["bytes"] for t in manifest["tiles"]) + len(g9)
    (OUT / "manifest.json").write_text(json.dumps(manifest, separators=(",", ":")) + "\n")
    print(f"wrote {kept:,} G<={G_LIMIT} records in {len(buckets)} tiles "
          f"({manifest['totalBytes']:,} bytes), {manifest['g9']['count']:,} in g9.bin, "
          f"dropped {dropped:,} already in the bright catalog")
    return manifest


def verify() -> None:
    manifest = json.loads((OUT / "manifest.json").read_text())
    for tile in manifest["tiles"] + [manifest["g9"]]:
        blob = (OUT / tile["file"]).read_bytes()
        assert blob[:8] == MAGIC, tile["file"]
        assert HEADER.unpack_from(blob, 8)[0] == tile["count"], tile["file"]
        assert len(blob) == tile["bytes"], tile["file"]
        assert sha256_of(blob) == tile["sha256"], tile["file"]
    print(f"verified {len(manifest['tiles'])} tiles + g9.bin against the manifest")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--from-legacy", type=Path,
                        default=Path(os.environ.get("RN_LEGACY_LOD", ROOT / "data" / "legacy")))
    parser.add_argument("--from-tap", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.verify_only:
        verify()
        return
    if args.from_tap:
        buckets, source = buckets_from_tap(ROOT / "data" / "tap" / "lod")
    else:
        buckets, source = buckets_from_legacy(args.from_legacy)
    build(buckets, source.get("release", "Gaia DR3"))
    verify()


if __name__ == "__main__":
    main()
