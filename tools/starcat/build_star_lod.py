#!/usr/bin/env python3
"""Build reproducible own-origin Gaia DR3 LOD assets.

The output uses a 24x32 equal-angle sky grid (768 addressable regions). Records
are grouped by tile so HTTP Range requests can fetch only the current view.
Run manually when refreshing the pinned Gaia DR3 source; production never
contacts ESA.
"""

import csv
import io
import json
import math
import os
import struct
import tempfile
import urllib.parse
import urllib.request
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "public" / "stars" / "lod"
TAP = "https://gea.esac.esa.int/tap-server/tap/sync"
QUERY = """SELECT source_id,ra,dec,phot_g_mean_mag,bp_rp
FROM gaiadr3.gaia_source
WHERE phot_g_mean_mag <= 12
  AND ra >= {ra_lo} AND ra < {ra_hi}
  AND dec >= {dec_lo} AND dec < {dec_hi}"""
MAGIC = b"GDR3LOD1"
RECORD = struct.Struct("<Qhhhh")
RA_BINS, DEC_BINS = 32, 24


def oct_encode(ra_deg, dec_deg):
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    x, y, z = math.cos(dec) * math.cos(ra), math.cos(dec) * math.sin(ra), math.sin(dec)
    scale = abs(x) + abs(y) + abs(z)
    x, y, z = x / scale, y / scale, z / scale
    if z < 0:
        x, y = (1 - abs(y)) * (1 if x >= 0 else -1), (1 - abs(x)) * (1 if y >= 0 else -1)
    return round(x * 32767), round(y * 32767)


def tile_for(ra, dec):
    col = min(RA_BINS - 1, int((ra % 360) / 360 * RA_BINS))
    row = min(DEC_BINS - 1, max(0, int((dec + 90) / 180 * DEC_BINS)))
    return row * RA_BINS + col


def download_csv(args):
    path, tile, ra_lo, ra_hi, dec_lo, dec_hi = args
    if path.exists() and path.stat().st_size > 30:
        return path
    query = QUERY.format(ra_lo=ra_lo, ra_hi=ra_hi, dec_lo=dec_lo, dec_hi=dec_hi)
    payload = urllib.parse.urlencode({"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv", "QUERY": query}).encode()
    request = urllib.request.Request(TAP, data=payload)
    for attempt in range(5):
        try:
            temporary = path.with_suffix(".part")
            with urllib.request.urlopen(request, timeout=180) as response, temporary.open("wb") as target:
                while chunk := response.read(1024 * 1024):
                    target.write(chunk)
            temporary.replace(path)
            print(f"Downloaded tile {tile:03}", flush=True)
            return path
        except Exception:
            if attempt == 4:
                raise
            time.sleep(2 ** attempt)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    cache = Path(os.environ.get("RN_GAIA_CACHE", ROOT / "target" / "gaia-cache"))
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="rn-gaia-") as temp:
        supplied = os.environ.get("RN_GAIA_CSV")
        if supplied:
            sources = [Path(supplied)]
        else:
            print("Downloading Gaia DR3 G<=12 in bounded sky tiles…", flush=True)
            jobs = []
            for row in range(DEC_BINS):
                for col in range(RA_BINS):
                    tile = row * RA_BINS + col
                    jobs.append((
                        cache / f"gaia-{tile:03}.csv",
                        tile,
                        col * 360 / RA_BINS,
                        (col + 1) * 360 / RA_BINS,
                        row * 180 / DEC_BINS - 90,
                        (row + 1) * 180 / DEC_BINS - 90,
                    ))
            with ThreadPoolExecutor(max_workers=12) as pool:
                sources = list(pool.map(download_csv, jobs))
        buckets = [bytearray() for _ in range(RA_BINS * DEC_BINS)]
        mid = bytearray()
        count = 0
        for source in sources:
            with source.open("rb") as raw:
                rows = csv.DictReader(io.TextIOWrapper(raw, encoding="utf-8", newline=""))
                for row in rows:
                    if not row["bp_rp"]:
                        color = 0
                    else:
                        color = round(max(-3.0, min(6.0, float(row["bp_rp"]))) * 1000)
                    ra, dec, mag = float(row["ra"]), float(row["dec"]), float(row["phot_g_mean_mag"])
                    ox, oy = oct_encode(ra, dec)
                    record = RECORD.pack(int(row["source_id"]), ox, oy, round(mag * 1000), color)
                    buckets[tile_for(ra, dec)].extend(record)
                    if mag <= 9:
                        mid.extend(record)
                    count += 1
                    if count % 250_000 == 0:
                        print(f"{count:,} stars", flush=True)

        (OUT / "g9.bin").write_bytes(MAGIC + struct.pack("<I", len(mid) // RECORD.size) + mid)
        manifest = {"version": 1, "release": "Gaia DR3", "recordBytes": RECORD.size, "raBins": RA_BINS, "decBins": DEC_BINS, "gLimit": 12, "tiles": []}
        offset = len(MAGIC) + 4
        with (OUT / "g12.bin").open("wb") as output:
            output.write(MAGIC); output.write(struct.pack("<I", count))
            for tile, data in enumerate(buckets):
                output.write(data)
                manifest["tiles"].append({"id": tile, "offset": offset, "bytes": len(data), "count": len(data) // RECORD.size})
                offset += len(data)
        (OUT / "manifest.json").write_text(json.dumps(manifest, separators=(",", ":")) + "\n")
        print(f"Wrote {count:,} G<=12 stars and {len(mid)//RECORD.size:,} G<=9 stars")


if __name__ == "__main__":
    main()
