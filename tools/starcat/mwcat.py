#!/usr/bin/env python3
"""Bake the sky's Milky Way texture from Gaia DR3 star counts.

Carried from the legacy universe site (`tools/mwcat.py`) and re-baked at
HEALPix level 9. The band behind the bright stars is not a painted nebula and
not procedural noise: it is 36.8 million real Gaia sources, binned onto the sky
and turned into surface brightness. Where the galactic disc is edge-on there
are more stars per unit area, so the band emerges on its own; where interstellar
dust blocks the light behind it the counts collapse, so the Great Rift appears
for free. The gold cast is measured, not styled — flux-weighted BP-RP colour
runs about 1.38 in the plane against 1.01 at the galactic poles, which is
interstellar reddening.

Level 7 (0.46° pixels) into a 1024x512 map was the legacy bake, and at 2x DPR
with the galactic centre filling the frame each pixel covers about 40 screen
pixels: the dust lanes read as blobs because the lattice, not the dust, is what
is resolved. Level 9 is NSIDE 512, 3,145,728 pixels of 0.115°, into 4096x2048 —
about 0.088° per texel, so the texel is finer than the datum and the shader
interpolates real structure rather than magnifying a cell.

Source — one ESA Gaia TAP aggregation, cut into 192 indexed source_id ranges:

    SELECT t.hpx9, COUNT(*) AS n, SUM(t.f * t.c) / SUM(t.f) AS bp_rp_w
    FROM (SELECT source_id/2199023255552 AS hpx9,
                 phot_g_mean_flux AS f, bp_rp AS c
          FROM gaiadr3.gaia_source
          WHERE phot_g_mean_mag < 15 AND bp_rp IS NOT NULL
            AND source_id >= :lo AND source_id < :hi) AS t
    GROUP BY t.hpx9

Two things about that query are worth knowing before editing it. Gaia's
`source_id` encodes the source's level-12 NESTED HEALPix index in its high
bits, so integer division by 2^(35 + 2*(12 - N)) yields the level-N index
without any coordinate maths — 2199023255552 is 2^41, that divisor for level 9.
And ADQL will not `GROUP BY` an expression, nor does this service expose
`gaia_healpix_index`, hence the derived table with an alias. The HEALPix grid
Gaia's source_id encodes is in ICRS, so pixel centres are equatorial and no
galactic transform is needed here or in the shader.

The ~120 MB CSV that query returns is an intermediate, not an artifact: it is
written to data/ (gitignored) and only the WebP pair is committed, alongside
public/sky/source.json recording the query, the row count and the checksums.

Output `public/sky/milkyway-<hash>.webp` — equirectangular in equatorial J2000,
u = ra/2pi + 0.5, v = 0.5 - dec/pi, matching the shader's sampling — plus a
2048x1024 downscale for low-DPR and phone viewports. The name carries the
content hash so a new bake is a new URL and no cache anywhere has to be purged.
"""

import argparse
import concurrent.futures as futures
import csv
import hashlib
import io
import json
import math
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import gaussian_filter

sys.path.insert(0, str(Path(__file__).resolve().parent))
from healpix import ang2pix_nest, check_galactic_geometry, selftest

ROOT = Path(__file__).resolve().parent.parent.parent
CATALOG = ROOT / "data" / "gaia_mw_hpx9.csv"
OUT_DIR = ROOT / "public" / "sky"
SOURCE_JSON = OUT_DIR / "source.json"

TAP = "https://gea.esac.esa.int/tap-server/tap"
ORDER = 9
NSIDE = 1 << ORDER
NPIX = 12 * NSIDE * NSIDE
DIVISOR = 1 << (35 + 2 * (12 - ORDER))  # 2199023255552
WIDTH, HEIGHT = 4096, 2048
SMALL = (2048, 1024)
# Level-9 texels are finer than the HEALPix pixel, so supersampling is only
# anti-aliasing the cell boundary; 4x4 would be 134M lookups for no more signal.
SUPERSAMPLE = 2
ROWS_PER_CHUNK = 64  # bounds the resample working set to ~200 MB

# 192 chunks is one level-4 superpixel each: 16,384 level-9 pixels, about
# 16k rows and half a megabyte of CSV, which the archive delivers intact.
CHUNKS = 192
CHUNK_DIR = ROOT / "data" / "tap" / "mw"

QUERY = f"""SELECT t.hpx{ORDER}, COUNT(*) AS n, SUM(t.f * t.c) / SUM(t.f) AS bp_rp_w
FROM (SELECT source_id/{DIVISOR} AS hpx{ORDER},
             phot_g_mean_flux AS f, bp_rp AS c
      FROM gaiadr3.gaia_source
      WHERE phot_g_mean_mag < 15 AND bp_rp IS NOT NULL
        AND source_id >= {{lo}} AND source_id < {{hi}}) AS t
GROUP BY t.hpx{ORDER}"""

# Everything below the diffuse floor is resolved foreground stars, not galactic
# glow: at high galactic latitude Gaia still counts stars in every pixel, and
# rendering that as a uniform wash reads as overcast cloud rather than sky.
# Subtracting the 35th percentile puts the high-latitude sky at zero and leaves
# the band as the only thing that survives.
FLOOR_PERCENTILE = 35.0
PEAK_PERCENTILE = 99.6

# BP-RP colour of the flux, mapped to a display ramp. The plane really is
# redder than the poles; this widens that measured difference into something a
# screen can show, rather than inventing a hue that isn't there.
COOL = np.array([0.72, 0.78, 0.95])  # bp_rp <= 0.9, out of the plane
WARM = np.array([1.00, 0.88, 0.66])  # bp_rp >= 1.5, deep in the plane


def chunk_bounds(chunk: int) -> tuple[int, int]:
    """The source_id range of one level-4 superpixel.

    A NESTED index truncates to its parent by shifting, so chunk `c` is exactly
    the pixels `[c * PER_CHUNK, (c+1) * PER_CHUNK)` and — because the index is
    the top bits of `source_id` — a contiguous, *indexed* source_id range. The
    archive answers each in a few seconds and never has to scan the catalogue.
    """
    per = NPIX // CHUNKS
    return chunk * per * DIVISOR, (chunk + 1) * per * DIVISOR


def fetch_chunk(chunk: int) -> int:
    out = CHUNK_DIR / f"mw-{chunk:03}.csv"
    if out.exists() and out.stat().st_size > 30:
        return 0
    lo, hi = chunk_bounds(chunk)
    payload = urllib.parse.urlencode({
        "REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv",
        "QUERY": QUERY.format(lo=lo, hi=hi),
    }).encode()
    for attempt in range(8):
        try:
            request = urllib.request.Request(f"{TAP}/sync", data=payload)
            with urllib.request.urlopen(request, timeout=600) as response:
                body = response.read()
            if not body.startswith(f"hpx{ORDER}".encode()):
                raise RuntimeError(body[:200].decode("utf-8", "replace"))
            part = out.with_suffix(".part")
            part.write_bytes(body)
            part.replace(out)
            return body.count(b"\n") - 1
        except Exception as error:  # noqa: BLE001 - the archive resets often
            if attempt == 7:
                raise
            print(f"  chunk {chunk}: {error}; retry", flush=True)
            time.sleep(min(60, 2 ** attempt))
    raise RuntimeError("unreachable")


def fetch_catalog(path: Path) -> None:
    """Pull every chunk, then concatenate them into one CSV.

    One whole-sky query is the honest statement of what is wanted, and the
    archive will run it — but it will not *deliver* it: the result stream for
    three million rows is reset by the front end after a couple of megabytes,
    with no Range support to resume from. Splitting it along the HEALPix index
    asks the same question 192 times over ranges the server can answer without
    a scan, and each answer arrives whole.
    """
    CHUNK_DIR.mkdir(parents=True, exist_ok=True)
    done = 0
    with futures.ThreadPoolExecutor(max_workers=4) as pool:
        for _ in pool.map(fetch_chunk, range(CHUNKS)):
            done += 1
            if done % 24 == 0:
                print(f"  {done}/{CHUNKS} chunks", flush=True)

    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_suffix(".partial")
    with partial.open("wb") as out:
        out.write(f"hpx{ORDER},n,bp_rp_w\n".encode())
        for chunk in range(CHUNKS):
            body = (CHUNK_DIR / f"mw-{chunk:03}.csv").read_bytes()
            out.write(body.split(b"\n", 1)[1])
    partial.rename(path)
    print(f"wrote {path} ({path.stat().st_size:,} bytes)", flush=True)


def read_catalog(path: Path) -> tuple[np.ndarray, np.ndarray, int]:
    """Return per-pixel (star count, flux-weighted BP-RP) and the row count.

    Counts rather than summed flux carry the intensity. Summed flux is
    dominated by whichever single star in the pixel happens to be brightest —
    a heavy tail that bakes as salt-and-pepper speckle over the whole sky.
    Counts are the honest proxy for integrated starlight at a fixed depth
    anyway, and they are what makes the Great Rift read as a shadow.
    """
    counts = np.zeros(NPIX, dtype=np.float64)
    colour = np.full(NPIX, np.nan, dtype=np.float64)
    rows = 0
    with path.open(newline="") as handle:
        reader = csv.reader(handle)
        header = next(reader)
        ipix, icount, icolour = (
            header.index(f"hpx{ORDER}"),
            header.index("n"),
            header.index("bp_rp_w"),
        )
        for row in reader:
            pixel = int(row[ipix])
            counts[pixel] = float(row[icount])
            if row[icolour]:
                colour[pixel] = float(row[icolour])
            rows += 1
    covered = int(np.count_nonzero(counts))
    # At level 9 a high-latitude pixel holds a handful of stars, so a few empty
    # ones are Poisson, not a broken pull; a real failure loses whole faces.
    if covered < NPIX * 0.95:
        raise SystemExit(f"catalog covers only {covered:,}/{NPIX:,} pixels")
    print(f"{rows:,} rows, {covered:,}/{NPIX:,} pixels covered", flush=True)
    return counts, colour, rows


def resample(pixel_values: np.ndarray, fill: float) -> np.ndarray:
    """HEALPix map -> equirectangular grid, supersampled per texel.

    The sample grid matches the shader's uv convention exactly:
    u = ra/2pi + 0.5, v = 0.5 - dec/pi. Supersampling averages the HEALPix
    pixels a texel straddles instead of point-sampling one of them, which is
    what keeps the cell lattice off the map. Done in row blocks because the
    full 8192x4096 sample grid would be several gigabytes of int64 at once.
    """
    out = np.empty((HEIGHT, WIDTH), dtype=np.float64)
    u = (np.arange(WIDTH * SUPERSAMPLE) + 0.5) / (WIDTH * SUPERSAMPLE)
    ra = (u - 0.5) * 2.0 * math.pi
    for start in range(0, HEIGHT, ROWS_PER_CHUNK):
        stop = min(start + ROWS_PER_CHUNK, HEIGHT)
        v = (np.arange(start * SUPERSAMPLE, stop * SUPERSAMPLE) + 0.5) / (
            HEIGHT * SUPERSAMPLE
        )
        dec = (0.5 - v) * math.pi
        ra_grid, dec_grid = np.meshgrid(ra, dec)
        pixels = ang2pix_nest(
            math.pi / 2.0 - dec_grid.ravel(), ra_grid.ravel(), ORDER
        )
        fine = np.nan_to_num(pixel_values[pixels], nan=fill)
        fine = fine.reshape(stop - start, SUPERSAMPLE, WIDTH, SUPERSAMPLE)
        out[start:stop] = fine.mean(axis=(1, 3))
    return out


def latitude_blur(field: np.ndarray, sigma_deg: float) -> np.ndarray:
    """Blur by a fixed angle on the sphere, not a fixed number of texels.

    Equirectangular rows near the poles cover far less sky per texel, so a
    uniform horizontal blur under-smooths there and leaves the streaked
    lattice this projection is notorious for. Scaling the horizontal sigma by
    1/cos(dec) makes the kernel isotropic in angle.
    """
    dec = np.radians((0.5 - (np.arange(HEIGHT) + 0.5) / HEIGHT) * 180.0)
    sigma_y = sigma_deg * HEIGHT / 180.0
    out = gaussian_filter(field, sigma=(sigma_y, 0.0), mode="nearest")
    scale = np.clip(1.0 / np.maximum(np.cos(dec), 1e-3), 1.0, 60.0)
    for row in range(HEIGHT):
        out[row] = gaussian_filter(
            out[row], sigma=sigma_deg * WIDTH / 360.0 * scale[row], mode="wrap"
        )
    return out


def bake(counts: np.ndarray, colour: np.ndarray, sharpness: float) -> Image.Image:
    # Percentiles are taken on the HEALPix array, not the resampled image:
    # HEALPix pixels are equal-area, so every pixel is one equal vote, while
    # an equirectangular image counts the polar sky many times over.
    floor = np.percentile(counts, FLOOR_PERCENTILE)
    peak = np.percentile(np.maximum(counts - floor, 0.0), PEAK_PERCENTILE)

    intensity = np.clip((resample(counts, floor) - floor) / peak, 0.0, 1.0)
    intensity = latitude_blur(intensity, sharpness)
    tint = latitude_blur(resample(colour, float(np.nanmedian(colour))), 2.0)

    # Store roughly perceptually; the shader's pow(tex, shape) undoes this and
    # then some, which is where the final contrast is dialled in.
    stored = intensity ** (1.0 / 2.2)
    warmth = np.clip((tint - 0.9) / 0.6, 0.0, 1.0)[..., None]
    rgb = COOL * (1.0 - warmth) + WARM * warmth
    return Image.fromarray(
        np.clip(stored[..., None] * rgb * 255.0, 0, 255).astype(np.uint8)
    )


def encode(image: Image.Image, quality: int) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="WEBP", quality=quality, method=6)
    return buffer.getvalue()


def write_outputs(image: Image.Image, quality: int, rows: int) -> None:
    """Write the hashed pair and the provenance record, dropping any earlier
    bake. The hash is in the filename so a re-bake is a new URL: no CDN purge,
    no stale band on a visitor who has the old one cached."""
    full = encode(image, quality)
    digest = hashlib.sha256(full).hexdigest()
    stub = digest[:12]
    small = encode(image.resize(SMALL, Image.LANCZOS), quality)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for stale in OUT_DIR.glob("milkyway*.webp"):
        stale.unlink()
    big_path = OUT_DIR / f"milkyway-{stub}.webp"
    small_path = OUT_DIR / f"milkyway-2k-{stub}.webp"
    big_path.write_bytes(full)
    small_path.write_bytes(small)

    SOURCE_JSON.write_text(
        json.dumps(
            {
                "catalog": "Gaia DR3 (ESA/Gaia/DPAC, CC BY-SA 3.0 IGO)",
                "tap": f"{TAP}/sync",
                "query": QUERY,
                "chunks": CHUNKS,
                "healpix_order": ORDER,
                "healpix_divisor": DIVISOR,
                "rows": rows,
                "projection": "equirectangular, equatorial J2000",
                "built": time.strftime("%Y-%m-%d"),
                "maps": [
                    {
                        "file": big_path.name,
                        "size": [WIDTH, HEIGHT],
                        "bytes": len(full),
                        "sha256": digest,
                    },
                    {
                        "file": small_path.name,
                        "size": list(SMALL),
                        "bytes": len(small),
                        "sha256": hashlib.sha256(small).hexdigest(),
                    },
                ],
            },
            indent=2,
        )
        + "\n"
    )
    print(f"wrote {big_path.name} ({len(full):,} B) + {small_path.name} ({len(small):,} B)")


def main() -> None:
    parser = argparse.ArgumentParser(description="bake the Milky Way band")
    parser.add_argument("--catalog", type=Path, default=CATALOG)
    parser.add_argument("--fetch-only", action="store_true")
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--quality", type=int, default=88)
    parser.add_argument(
        "--sharpness",
        type=float,
        default=0.16,
        help="angular blur sigma in degrees; level 9 resolves finer than 0.45",
    )
    args = parser.parse_args()

    if args.selftest:
        print(f"ang2pix_nest ok (equal-area spread {selftest(ORDER):.3f})")
        return
    if not args.catalog.exists():
        fetch_catalog(args.catalog)
    if args.fetch_only:
        return

    counts, colour, rows = read_catalog(args.catalog)
    check_galactic_geometry(counts, ORDER)
    plane = np.nanmedian(colour[counts > np.percentile(counts, 95)])
    poles = np.nanmedian(colour[counts < np.percentile(counts, 20)])
    print(f"flux-weighted BP-RP: plane {plane:.3f}, out of plane {poles:.3f}")

    write_outputs(bake(counts, colour, args.sharpness), args.quality, rows)


if __name__ == "__main__":
    main()
