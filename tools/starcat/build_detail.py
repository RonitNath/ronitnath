#!/usr/bin/env python3
"""Build the offline star-detail dataset for the sky panel (plan S3).

Every upstream is pulled here, once. The web app and its API never contact ESA
or CDS at runtime; they read `sky_star_detail` from Postgres.

Stages (`--stage`, repeatable, default `all`):
  gaia    Gaia DR3 gaia_source LEFT JOIN astrophysical_parameters, G <= 12,
          768 sky tiles, resumable under data/tap/gaia/
  hyg     HYG v3 (astronexus/HYG-Database, CC BY-SA 4.0)
  iau     IAU WGSN Catalog of Star Names
  bounds  IAU constellation boundaries (VizieR VI/42, Roman 1987)
  simbad  SIMBAD TAP basic+ident for the bright catalog ids only
  merge   one JSON payload per star -> data/sky_star_detail.csv.zst

Python 3 standard library only.
"""

from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import io
import json
import math
import os
import random
import re
import shutil
import struct
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
DATA = Path(os.environ.get("RN_SKY_DATA", ROOT / "data"))
TAP_CACHE = DATA / "tap"
GAIA_TAP = "https://gea.esac.esa.int/tap-server/tap/sync"
SIMBAD_TAP = "https://simbad.cds.unistra.fr/simbad/sim-tap/sync"
VIZIER_TAP = "https://tapvizier.cds.unistra.fr/TAPVizieR/tap/sync"
RA_BINS, DEC_BINS = 32, 24
USER_AGENT = "ronitnath.com sky builder (offline dataset build; contact ronit@isoastra.com)"

GAIA_COLUMNS = [
    "source_id", "ra", "dec", "parallax", "parallax_error", "pmra", "pmdec",
    "radial_velocity", "ruwe", "phot_g_mean_mag", "phot_bp_mean_mag",
    "phot_rp_mean_mag", "bp_rp", "phot_variable_flag", "non_single_star",
]
AP_COLUMNS = [
    "teff_gspphot", "distance_gspphot", "radius_flame", "lum_flame",
    "mass_flame", "age_flame",
]
GAIA_QUERY = (
    "SELECT " + ",".join("g." + c for c in GAIA_COLUMNS)
    + "," + ",".join("a." + c for c in AP_COLUMNS)
    + " FROM gaiadr3.gaia_source AS g"
    " LEFT OUTER JOIN gaiadr3.astrophysical_parameters AS a"
    " ON g.source_id = a.source_id"
    " WHERE g.phot_g_mean_mag <= 12"
    " AND g.ra >= {ra_lo} AND g.ra < {ra_hi}"
    " AND g.dec >= {dec_lo} AND g.dec < {dec_hi}"
)

HYG_BASE = "https://raw.githubusercontent.com/astronexus/HYG-Database/main/"
HYG_URLS = [HYG_BASE + "hyg/v3/hyg_v38.csv.gz", HYG_BASE + "hyg/v3/hyg_v37.csv.gz"]
IAU_URLS = ["https://www.pas.rochester.edu/~emamajek/WGSN/IAU-CSN.txt"]
BOUNDS_URL = "https://cdsarc.cds.unistra.fr/ftp/VI/42/data.dat"

CONSTELLATION_NAMES = {
    "And": "Andromeda", "Ant": "Antlia", "Aps": "Apus", "Aqr": "Aquarius",
    "Aql": "Aquila", "Ara": "Ara", "Ari": "Aries", "Aur": "Auriga",
    "Boo": "Bootes", "Cae": "Caelum", "Cam": "Camelopardalis", "Cnc": "Cancer",
    "CVn": "Canes Venatici", "CMa": "Canis Major", "CMi": "Canis Minor",
    "Cap": "Capricornus", "Car": "Carina", "Cas": "Cassiopeia", "Cen": "Centaurus",
    "Cep": "Cepheus", "Cet": "Cetus", "Cha": "Chamaeleon", "Cir": "Circinus",
    "Col": "Columba", "Com": "Coma Berenices", "CrA": "Corona Australis",
    "CrB": "Corona Borealis", "Crv": "Corvus", "Crt": "Crater", "Cru": "Crux",
    "Cyg": "Cygnus", "Del": "Delphinus", "Dor": "Dorado", "Dra": "Draco",
    "Equ": "Equuleus", "Eri": "Eridanus", "For": "Fornax", "Gem": "Gemini",
    "Gru": "Grus", "Her": "Hercules", "Hor": "Horologium", "Hya": "Hydra",
    "Hyi": "Hydrus", "Ind": "Indus", "Lac": "Lacerta", "Leo": "Leo",
    "LMi": "Leo Minor", "Lep": "Lepus", "Lib": "Libra", "Lup": "Lupus",
    "Lyn": "Lynx", "Lyr": "Lyra", "Men": "Mensa", "Mic": "Microscopium",
    "Mon": "Monoceros", "Mus": "Musca", "Nor": "Norma", "Oct": "Octans",
    "Oph": "Ophiuchus", "Ori": "Orion", "Pav": "Pavo", "Peg": "Pegasus",
    "Per": "Perseus", "Phe": "Phoenix", "Pic": "Pictor", "Psc": "Pisces",
    "PsA": "Piscis Austrinus", "Pup": "Puppis", "Pyx": "Pyxis",
    "Ret": "Reticulum", "Sge": "Sagitta", "Sgr": "Sagittarius", "Sco": "Scorpius",
    "Scl": "Sculptor", "Sct": "Scutum", "Ser": "Serpens", "Sex": "Sextans",
    "Tau": "Taurus", "Tel": "Telescopium", "Tri": "Triangulum",
    "TrA": "Triangulum Australe", "Tuc": "Tucana", "UMa": "Ursa Major",
    "UMi": "Ursa Minor", "Vel": "Vela", "Vir": "Virgo", "Vol": "Volans",
    "Vul": "Vulpecula",
}

_log_lock = threading.Lock()


def log(message: str) -> None:
    stamp = time.strftime("%H:%M:%S")
    with _log_lock:
        print(f"[{stamp}] {message}", flush=True)
        path = DATA / "BUILD_LOG.md"
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open("a", encoding="utf-8") as handle:
                handle.write(f"- {time.strftime('%Y-%m-%d %H:%M:%S')} {message}\n")
        except OSError:
            pass


def http(url: str, data: bytes | None = None, timeout: int = 600,
         attempts: int = 6) -> bytes:
    """GET/POST with backoff on 429 and 5xx."""
    for attempt in range(attempts):
        request = urllib.request.Request(url, data=data, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            retryable = error.code == 429 or error.code >= 500
            if not retryable or attempt == attempts - 1:
                raise
            wait = min(120, 2 ** attempt) + random.random() * 2
            log(f"HTTP {error.code} on {url.split('?')[0]}; retry in {wait:.0f}s")
            time.sleep(wait)
        except Exception as error:  # noqa: BLE001 - network flakiness
            if attempt == attempts - 1:
                raise
            wait = min(120, 2 ** attempt) + random.random() * 2
            log(f"{type(error).__name__} on {url.split('?')[0]}; retry in {wait:.0f}s")
            time.sleep(wait)
    raise RuntimeError("unreachable")


def tap_sync(endpoint: str, query: str, fmt: str = "csv") -> bytes:
    payload = urllib.parse.urlencode({
        "REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": fmt, "QUERY": query,
    }).encode()
    return http(endpoint, data=payload)


def tile_bounds(tile: int) -> tuple[float, float, float, float]:
    row, col = divmod(tile, RA_BINS)
    return (
        col * 360 / RA_BINS, (col + 1) * 360 / RA_BINS,
        row * 180 / DEC_BINS - 90, (row + 1) * 180 / DEC_BINS - 90,
    )


# ---------------------------------------------------------------- stage: gaia

def fetch_gaia_tile(tile: int) -> int:
    out = TAP_CACHE / "gaia" / f"gaia-{tile:03}.csv"
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists() and out.stat().st_size > 30:
        return 0
    ra_lo, ra_hi, dec_lo, dec_hi = tile_bounds(tile)
    query = GAIA_QUERY.format(ra_lo=ra_lo, ra_hi=ra_hi, dec_lo=dec_lo, dec_hi=dec_hi)
    body = tap_sync(GAIA_TAP, query)
    if not body.startswith(b"source_id"):
        head = body[:200].decode("utf-8", "replace")
        raise RuntimeError(f"tile {tile}: unexpected TAP response: {head}")
    part = out.with_suffix(".part")
    part.write_bytes(body)
    part.replace(out)
    return body.count(b"\n") - 1


def stage_gaia(workers: int = 4) -> None:
    (TAP_CACHE / "gaia").mkdir(parents=True, exist_ok=True)
    todo = [t for t in range(RA_BINS * DEC_BINS)
            if not (TAP_CACHE / "gaia" / f"gaia-{t:03}.csv").exists()]
    log(f"gaia: {len(todo)} tiles to fetch ({768 - len(todo)} cached)")
    done = 0
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for rows in pool.map(fetch_gaia_tile, todo):
            done += 1
            if done % 16 == 0 or done == len(todo):
                log(f"gaia: {done}/{len(todo)} tiles")
    log("gaia: complete")


# ----------------------------------------------------------- stage: hyg / iau

def stage_hyg() -> Path:
    out = TAP_CACHE / "hyg_v3.csv"
    if out.exists() and out.stat().st_size > 1000:
        log(f"hyg: cached {out.stat().st_size:,} bytes")
        return out
    for url in HYG_URLS:
        try:
            body = http(url, timeout=300, attempts=3)
            if url.endswith(".gz"):
                body = gzip.decompress(body)
        except Exception as error:  # noqa: BLE001
            log(f"hyg: {url} failed ({error})")
            continue
        if b"proper" in body[:4000]:
            out.write_bytes(body)
            log(f"hyg: {url} -> {len(body):,} bytes uncompressed")
            return out
    raise RuntimeError("no HYG source reachable")


def stage_iau() -> Path:
    out = TAP_CACHE / "iau_csn.txt"
    if out.exists() and out.stat().st_size > 1000:
        log("iau: cached")
        return out
    for url in IAU_URLS:
        try:
            body = http(url, timeout=120, attempts=2)
        except Exception as error:  # noqa: BLE001
            log(f"iau: {url} failed ({error})")
            continue
        if b"HIP" in body:
            out.write_bytes(body)
            log(f"iau: {url} -> {len(body):,} bytes")
            return out
    raise RuntimeError("no IAU star-name source reachable")


# -------------------------------------------------------------- stage: bounds

def stage_bounds() -> Path:
    """VizieR VI/42 (Roman 1987): the IAU boundaries as B1875 RA/Dec arcs."""
    out = TAP_CACHE / "constellation_boundaries.csv"
    if out.exists() and out.stat().st_size > 1000:
        log("bounds: cached")
        return out
    body = http(BOUNDS_URL, attempts=4)
    rows = ["RAl,RAu,DEl,cst"]
    for line in body.decode("ascii", "replace").splitlines():
        if len(line) < 29:
            continue
        rows.append(f"{line[1:8].strip()},{line[9:16].strip()},"
                    f"{line[17:25].strip()},{line[26:29].strip()}")
    out.write_text("\n".join(rows) + "\n", encoding="utf-8")
    log(f"bounds: {len(rows) - 1} boundary arcs from VizieR VI/42")
    return out


def load_boundaries(path: Path) -> list[tuple[float, float, float, str]]:
    rows: list[tuple[float, float, float, str]] = []
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            try:
                rows.append((float(row["RAl"]), float(row["RAu"]),
                             float(row["DEl"]), row["cst"].strip()))
            except (KeyError, ValueError):
                continue
    if not rows:
        raise RuntimeError("empty constellation boundary table")
    return rows


def precess_to_b1875(ra_deg: float, dec_deg: float) -> tuple[float, float]:
    """ICRS/J2000 -> B1875 (the epoch the IAU boundaries are drawn in)."""
    t = (1875.0 - 2000.0) / 100.0  # tropical centuries from J2000
    zeta = (2306.2181 * t + 0.30188 * t * t + 0.017998 * t ** 3) / 3600.0
    z = (2306.2181 * t + 1.09468 * t * t + 0.018203 * t ** 3) / 3600.0
    theta = (2004.3109 * t - 0.42665 * t * t - 0.041833 * t ** 3) / 3600.0
    zeta, z, theta = map(math.radians, (zeta, z, theta))
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    x = math.cos(dec) * math.cos(ra)
    y = math.cos(dec) * math.sin(ra)
    w = math.sin(dec)
    cz, sz = math.cos(zeta), math.sin(zeta)
    ct, st = math.cos(theta), math.sin(theta)
    cZ, sZ = math.cos(z), math.sin(z)
    # inverse of the J2000 -> date rotation: R = Rz(zeta) Ry(-theta) Rz(z)
    x1, y1 = cZ * x + sZ * y, -sZ * x + cZ * y
    x2, w2 = ct * x1 - st * w, st * x1 + ct * w
    x3, y3 = cz * x2 + sz * y1, -sz * x2 + cz * y1
    ra_out = math.degrees(math.atan2(y3, x3)) % 360.0
    dec_out = math.degrees(math.asin(max(-1.0, min(1.0, w2))))
    return ra_out, dec_out


def constellation_for(ra_deg: float, dec_deg: float,
                      table: list[tuple[float, float, float, str]]) -> str | None:
    ra75, dec75 = precess_to_b1875(ra_deg, dec_deg)
    ra_h = ra75 / 15.0
    for ra_lo, ra_hi, dec_lo, abbr in table:
        if dec75 < dec_lo or ra_h < ra_lo or ra_h >= ra_hi:
            continue
        return abbr
    return None


# -------------------------------------------------------------- stage: simbad

def bright_ids() -> tuple[list[str], list[str]]:
    base = Path(os.environ.get("RN_BRIGHT_DIR", HERE / "data"))
    gaia_csv, hip_csv = base / "gaia_g6.5.csv", base / "hip_2.5.csv"
    with gaia_csv.open(newline="", encoding="utf-8") as handle:
        gaia = [row["source_id"] for row in csv.DictReader(handle)]
    with hip_csv.open(newline="", encoding="utf-8") as handle:
        hip = [row["hip"] for row in csv.DictReader(handle)]
    return gaia, hip


def simbad_batch(index: int, idents: list[str]) -> Path:
    out = TAP_CACHE / "simbad" / f"batch-{index:04}.csv"
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists() and out.stat().st_size > 20:
        return out
    quoted = ",".join("'" + i.replace("'", "''") + "'" for i in idents)
    query = (
        "SELECT i.id AS query_id, b.main_id, b.otype_txt, b.sp_type,"
        " b.plx_value, b.pmra, b.pmdec, b.rvz_radvel, a.id AS other_id"
        " FROM ident AS i"
        " JOIN basic AS b ON b.oid = i.oidref"
        " JOIN ident AS a ON a.oidref = i.oidref"
        f" WHERE i.id IN ({quoted})"
    )
    body = tap_sync(SIMBAD_TAP, query)
    if not body.lstrip().lower().startswith(b'"query_id"') and b"query_id" not in body[:200].lower():
        raise RuntimeError(f"simbad batch {index}: {body[:200]!r}")
    part = out.with_suffix(".part")
    part.write_bytes(body)
    part.replace(out)
    return out


def stage_simbad(batch_size: int = 500, workers: int = 3) -> None:
    (TAP_CACHE / "simbad").mkdir(parents=True, exist_ok=True)
    gaia, hip = bright_ids()
    idents = [f"Gaia DR3 {s}" for s in gaia] + [f"HIP {h}" for h in hip]
    batches = [idents[i:i + batch_size] for i in range(0, len(idents), batch_size)]
    log(f"simbad: {len(idents)} ids in {len(batches)} batches")
    with ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(lambda pair: simbad_batch(*pair), list(enumerate(batches))))
    log("simbad: complete")


def read_simbad() -> dict[str, dict]:
    merged: dict[str, dict] = {}
    for path in sorted((TAP_CACHE / "simbad").glob("batch-*.csv")):
        with path.open(newline="", encoding="utf-8") as handle:
            for row in csv.DictReader(handle):
                key = row["query_id"].strip()
                entry = merged.setdefault(key, {"ids": []})
                for field in ("main_id", "otype_txt", "sp_type", "plx_value",
                              "pmra", "pmdec", "rvz_radvel"):
                    value = (row.get(field) or "").strip()
                    if value and field not in entry:
                        entry[field] = value
                other = (row.get("other_id") or "").strip()
                if other and other not in entry["ids"]:
                    entry["ids"].append(other)
    return merged


# --------------------------------------------------------------- stage: merge

def number(value) -> float | None:
    if value is None:
        return None
    text = str(value).strip()
    if not text or text.lower() in {"nan", "null", "none"}:
        return None
    try:
        result = float(text)
    except ValueError:
        return None
    return None if math.isnan(result) or math.isinf(result) else result


def prune(payload: dict) -> dict:
    """Drop empty values and clip float precision.

    Six decimals is 3.6 mas on a position and far past the uncertainty on every
    other quantity here; carrying full float64 repr instead more than doubles
    the shipped dataset for no information.
    """
    out = {}
    for key, value in payload.items():
        if isinstance(value, float):
            value = round(value, 6)
        if value is None or value == "" or value == [] or value == {}:
            continue
        out[key] = value
    return out


def read_hyg(path: Path) -> tuple[dict[str, dict], HygIndex]:
    """HYG rows keyed by HIP, plus a positional index for the Gaia join."""
    by_hip: dict[str, dict] = {}
    positions: list[tuple[float, float, float, dict]] = []
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            hyg_ra, hyg_dec = number(row.get("ra")), number(row.get("dec"))
            entry = prune({
                "ra": None if hyg_ra is None else hyg_ra * 15.0,
                "dec": hyg_dec,
                "hip": row.get("hip") or None,
                "hd": row.get("hd") or None,
                "hr": row.get("hr") or None,
                "gliese": row.get("gl") or None,
                "bayerFlamsteed": (row.get("bf") or "").strip() or None,
                "bayer": row.get("bayer") or None,
                "flamsteed": row.get("flam") or None,
                "proper": (row.get("proper") or "").strip() or None,
                "spectralType": (row.get("spect") or "").strip() or None,
                "constellation": (row.get("con") or "").strip() or None,
                "magV": number(row.get("mag")),
                "absMagV": number(row.get("absmag")),
                "colorIndex": number(row.get("ci")),
                "luminosity": number(row.get("lum")),
                "distanceParsecs": number(row.get("dist")),
            })
            if row.get("hip"):
                by_hip[str(int(float(row["hip"])))] = entry
            ra, dec = number(row.get("ra")), number(row.get("dec"))
            if ra is not None and dec is not None and row.get("id") != "0":
                positions.append((ra * 15.0, dec, entry.get("magV"), entry))
    return by_hip, HygIndex(positions)


IAU_ROW = re.compile(
    r"^(\S+)\s+(\S+)\s+(.+?)\s+(\S+)\s+(\S+)\s+(\S{1,3})\s+(\S+)\s+(\S+)"
    r"\s+([-+]?[\d.]+|_)\s+(\S+)\s+(\S+)\s+(\S+)\s+([\d.]+)\s+([-+]?[\d.]+)"
    r"\s+(\d{4}-\d{2}-\d{2})")


class HygIndex:
    """Match a Gaia DR3 star to a HYG v3 row by sky position and magnitude.

    HYG carries no Gaia cross-id, so the join is positional: the Gaia position
    is walked back from epoch 2016.0 to J2000 with its own proper motion, then
    matched against HYG within `RADIUS` and 2 magnitudes (HYG's V against
    Gaia's G).
    """

    CELL = 0.25  # degrees
    RADIUS = 3.0 / 3600.0  # degrees
    EPOCH_DELTA = -16.0  # years, Gaia DR3 epoch 2016.0 -> J2000

    def __init__(self, rows: list[tuple[float, float, float, dict]]):
        self.cells: dict[tuple[int, int], list] = {}
        for ra, dec, mag, entry in rows:
            self.cells.setdefault(self._cell(ra, dec), []).append((ra, dec, mag, entry))

    @classmethod
    def _cell(cls, ra: float, dec: float) -> tuple[int, int]:
        return int(ra / cls.CELL), int((dec + 90.0) / cls.CELL)

    def match(self, ra: float, dec: float, mag: float | None,
              pmra: float | None, pmdec: float | None) -> dict | None:
        if pmra is not None and pmdec is not None and abs(dec) < 89.9:
            dec = dec + pmdec * self.EPOCH_DELTA / 3.6e6
            ra = (ra + pmra * self.EPOCH_DELTA / 3.6e6
                  / max(1e-6, math.cos(math.radians(dec)))) % 360.0
        cx, cy = self._cell(ra, dec)
        cos_dec = math.cos(math.radians(dec))
        best, best_distance = None, self.RADIUS
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for hra, hdec, hmag, entry in self.cells.get((cx + dx, cy + dy), ()):
                    if mag is not None and hmag is not None and abs(hmag - mag) > 2.0:
                        continue
                    delta_ra = ((hra - ra + 180.0) % 360.0 - 180.0) * cos_dec
                    distance = math.hypot(delta_ra, hdec - dec)
                    if distance < best_distance:
                        best, best_distance = entry, distance
        return best


def read_iau(path: Path) -> tuple[dict[str, dict], dict[str, dict]]:
    """IAU-CSN fixed-column text -> name records keyed by HIP and by HD."""
    by_hip: dict[str, dict] = {}
    by_hd: dict[str, dict] = {}
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line.strip() or line[0] in "#$":
            continue
        match = IAU_ROW.match(line)
        if not match:
            continue
        (name, diacritics, designation, greek_id, _greek, con, _comp, _wds,
         mag, _band, hip, hd, ra, dec, _date) = match.groups()
        blank = {"_", "-", ""}
        record = prune({
            "iauName": name,
            "iauNameDiacritics": diacritics if diacritics not in blank else None,
            "designation": designation if designation not in blank else None,
            "bayer": greek_id if greek_id not in blank else None,
            "constellation": con if con not in blank else None,
            "magV": number(mag),
            "hip": hip if hip not in blank else None,
            "hd": hd if hd not in blank else None,
            "ra": number(ra),
            "dec": number(dec),
        })
        if record.get("hip"):
            by_hip.setdefault(record["hip"], record)
        if record.get("hd"):
            by_hd.setdefault(record["hd"], record)
    return by_hip, by_hd


def open_writer(target: Path):
    if shutil.which("zstd"):
        final = target.with_suffix(target.suffix + ".zst") if not str(target).endswith(".zst") else target
        process = subprocess.Popen(["zstd", "-19", "-T0", "-q", "-f", "-o", str(final)],
                                   stdin=subprocess.PIPE)
        return io.TextIOWrapper(process.stdin, encoding="utf-8", newline=""), process, final
    final = Path(str(target).replace(".csv.zst", ".csv.gz"))
    handle = gzip.open(final, "wt", encoding="utf-8", newline="")
    return handle, None, final


def stage_merge() -> None:
    bounds = load_boundaries(TAP_CACHE / "constellation_boundaries.csv")
    hyg_by_hip, hyg_index = read_hyg(TAP_CACHE / "hyg_v3.csv")
    iau_by_hip, iau_by_hd = read_iau(TAP_CACHE / "iau_csn.txt")
    simbad = read_simbad()
    gaia_bright, hip_bright = bright_ids()
    bright_gaia = set(gaia_bright)

    stats = {
        "builtAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "rows": 0, "gaiaRows": 0, "hipRows": 0,
        "withAstrophysical": 0, "withHyg": 0, "withIau": 0, "withSimbad": 0,
        "withConstellation": 0, "hygRowsByHip": len(hyg_by_hip),
        "hygPositions": len(hyg_index.cells), "iauNames": len(iau_by_hip) + len(iau_by_hd),
        "simbadRows": len(simbad),
    }
    target = DATA / "sky_star_detail.csv.zst"
    handle, process, final = open_writer(target)
    writer = csv.writer(handle)
    hip_seen: dict[str, dict] = {}

    for tile in range(RA_BINS * DEC_BINS):
        path = TAP_CACHE / "gaia" / f"gaia-{tile:03}.csv"
        if not path.exists():
            continue
        with path.open(newline="", encoding="utf-8") as source:
            for row in csv.DictReader(source):
                sid = (row.get("source_id") or "").strip()
                if not sid:
                    continue
                ra, dec = number(row.get("ra")), number(row.get("dec"))
                astro = prune({k: number(row.get(k)) for k in AP_COLUMNS})
                payload = {
                    "kind": "gaia",
                    "sourceId": sid,
                    "ra": ra,
                    "dec": dec,
                    "gaia": prune({
                        "parallax": number(row.get("parallax")),
                        "parallaxError": number(row.get("parallax_error")),
                        "pmra": number(row.get("pmra")),
                        "pmdec": number(row.get("pmdec")),
                        "radialVelocity": number(row.get("radial_velocity")),
                        "ruwe": number(row.get("ruwe")),
                        "magG": number(row.get("phot_g_mean_mag")),
                        "magBp": number(row.get("phot_bp_mean_mag")),
                        "magRp": number(row.get("phot_rp_mean_mag")),
                        "bpRp": number(row.get("bp_rp")),
                        "variableFlag": (row.get("phot_variable_flag") or "").strip() or None,
                        "nonSingleStar": number(row.get("non_single_star")),
                    }),
                    "astrophysical": astro,
                }
                sources = ["Gaia DR3 gaia_source"]
                if astro:
                    sources.append("Gaia DR3 astrophysical_parameters")
                    stats["withAstrophysical"] += 1
                simbad_row = simbad.get(f"Gaia DR3 {sid}")
                hyg = None
                if ra is not None and dec is not None:
                    hyg = hyg_index.match(ra, dec, number(row.get("phot_g_mean_mag")),
                                          number(row.get("pmra")), number(row.get("pmdec")))
                hip = None
                if simbad_row:
                    for ident in simbad_row.get("ids", []):
                        if ident.startswith("HIP "):
                            hip = ident[4:].strip()
                            break
                if hip and not hyg:
                    hyg = hyg_by_hip.get(hip)
                if hyg:
                    payload["hyg"] = hyg
                    sources.append("HYG v3")
                    stats["withHyg"] += 1
                    hip = hip or hyg.get("hip")
                iau = (iau_by_hip.get(hip) if hip else None) or \
                      (iau_by_hd.get(str(hyg.get("hd"))) if hyg and hyg.get("hd") else None)
                if iau:
                    payload["iau"] = iau
                    payload["name"] = iau["iauName"]
                    sources.append("IAU WGSN Catalog of Star Names")
                    stats["withIau"] += 1
                elif hyg and hyg.get("proper"):
                    payload["name"] = hyg["proper"]
                if simbad_row:
                    payload["simbad"] = prune(simbad_row)
                    sources.append("SIMBAD (CDS)")
                    stats["withSimbad"] += 1
                if hip:
                    payload["hip"] = hip
                if ra is not None and dec is not None:
                    abbr = constellation_for(ra, dec, bounds)
                    if abbr:
                        payload["constellation"] = abbr
                        payload["constellationName"] = CONSTELLATION_NAMES.get(abbr, abbr)
                        stats["withConstellation"] += 1
                payload["sources"] = sources
                if sid in bright_gaia:
                    payload["bright"] = True
                writer.writerow([f"g{sid}", json.dumps(prune(payload), separators=(",", ":"))])
                stats["rows"] += 1
                stats["gaiaRows"] += 1
                if hip:
                    hip_seen[hip] = payload
                if stats["rows"] % 250_000 == 0:
                    log(f"merge: {stats['rows']:,} rows")

    # HIP-only rows from the bright catalog (stars Gaia does not carry).
    for hip in hip_bright:
        if hip in hip_seen:
            continue
        hyg = hyg_by_hip.get(hip)
        simbad_row = simbad.get(f"HIP {hip}")
        iau = iau_by_hip.get(hip) or (iau_by_hd.get(str(hyg.get("hd")))
                                      if hyg and hyg.get("hd") else None)
        payload = {"kind": "hip", "hip": hip, "bright": True,
                   "sources": ["Hipparcos (bright catalog)"]}
        if hyg:
            payload["hyg"] = hyg
            payload["sources"].append("HYG v3")
            stats["withHyg"] += 1
        if iau:
            payload["iau"] = iau
            payload["name"] = iau["iauName"]
            payload["sources"].append("IAU WGSN Catalog of Star Names")
            stats["withIau"] += 1
        elif hyg and hyg.get("proper"):
            payload["name"] = hyg["proper"]
        if simbad_row:
            payload["simbad"] = prune(simbad_row)
            payload["sources"].append("SIMBAD (CDS)")
            stats["withSimbad"] += 1
        ra = hyg.get("ra") if hyg else None
        dec = hyg.get("dec") if hyg else None
        if ra is None and iau:
            ra, dec = iau.get("ra"), iau.get("dec")
        if ra is not None and dec is not None:
            payload["ra"], payload["dec"] = ra, dec
            abbr = constellation_for(ra, dec, bounds)
            if abbr:
                payload["constellation"] = abbr
                payload["constellationName"] = CONSTELLATION_NAMES.get(abbr, abbr)
                stats["withConstellation"] += 1
        writer.writerow([f"h{hip}", json.dumps(prune(payload), separators=(",", ":"))])
        stats["rows"] += 1
        stats["hipRows"] += 1

    handle.close()
    if process is not None:
        process.wait()
        if process.returncode != 0:
            raise RuntimeError("zstd failed")
    digest = hashlib.sha256()
    with final.open("rb") as blob:
        for chunk in iter(lambda: blob.read(1 << 20), b""):
            digest.update(chunk)
    (DATA / "sky_star_detail.sha256").write_text(
        f"{digest.hexdigest()}  {final.name}\n", encoding="utf-8")
    stats["file"] = final.name
    stats["fileBytes"] = final.stat().st_size
    stats["sha256"] = digest.hexdigest()
    for key in ("withAstrophysical", "withHyg", "withIau", "withSimbad", "withConstellation"):
        stats[key + "Rate"] = round(stats[key] / max(1, stats["rows"]), 4)
    (DATA / "sky_star_detail.stats.json").write_text(
        json.dumps(stats, indent=2) + "\n", encoding="utf-8")
    log(f"merge: {stats['rows']:,} rows -> {final.name} ({stats['fileBytes']:,} bytes)")


def selftest() -> None:
    bounds = load_boundaries(TAP_CACHE / "constellation_boundaries.csv")
    cases = [("Sirius", 101.28715533, -16.71611586, "CMa"),
             ("Vega", 279.23473479, 38.78368896, "Lyr"),
             ("Polaris", 37.95456067, 89.26410897, "UMi"),
             ("Betelgeuse", 88.79293899, 7.40706400, "Ori")]
    failures = []
    for name, ra, dec, want in cases:
        got = constellation_for(ra, dec, bounds)
        status = "ok" if got == want else "FAIL"
        print(f"{status:4} {name:11} -> {got} (want {want})")
        if got != want:
            failures.append(name)
    if failures:
        sys.exit(f"constellation self-test failed: {', '.join(failures)}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", action="append", default=None,
                        choices=["gaia", "hyg", "iau", "bounds", "simbad", "merge",
                                 "selftest", "all"])
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    stages = args.stage or ["all"]
    if "all" in stages:
        stages = ["gaia", "hyg", "iau", "bounds", "simbad", "selftest", "merge"]
    TAP_CACHE.mkdir(parents=True, exist_ok=True)
    for stage in stages:
        log(f"stage {stage}: start")
        if stage == "gaia":
            stage_gaia(min(4, args.workers))
        elif stage == "hyg":
            stage_hyg()
        elif stage == "iau":
            stage_iau()
        elif stage == "bounds":
            stage_bounds()
        elif stage == "simbad":
            stage_simbad()
        elif stage == "selftest":
            selftest()
        elif stage == "merge":
            stage_merge()
    log("build_detail: done")


if __name__ == "__main__":
    main()
