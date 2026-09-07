# starcat — the offline sky datasets

Everything the sky draws or shows is pulled here, once, and shipped. The browser
and the API never contact ESA, CDS or GitHub at runtime. Python 3 standard
library only; no virtualenv, no pip.

Bulk artifacts live under `data/`, which is gitignored. The LOD tiles under
`public/stars/lod/` are the exception: they are the shipped assets and are
committed.

| Script | Builds | Committed output |
| --- | --- | --- |
| `build_star_lod.py` | S2 streamed depth | `public/stars/lod/*.bin`, `manifest.json` |
| `build_detail.py` | S3 star detail | `data/sky_star_detail.sha256`, `.stats.json` |
| `load_detail.py` | the psql loader | `load_detail.sql` |

## build_star_lod.py — 768 tiles of Gaia DR3 G ≤ 12

A 32 × 24 equal-angle grid over the sky, one static file per tile, so the client
fetches only what the view needs.

- File: `b"GDR3LOD1"` + `u32` record count + records.
- Record (16 B, `<Qhhhh`): `source_id`, octahedral x, octahedral y,
  magnitude × 1000, (BP−RP) × 1000.
- `g9.bin` holds every G ≤ 9 record whole, for the fetch after first paint.
- `manifest.json` carries the grid, and per file a count, byte size and sha256.
- Records whose `source_id` is already in the bright STR2 catalog
  (`tools/starcat/data/gaia_g6.5.csv`) are dropped so nothing is drawn twice;
  the count lands in the manifest as `droppedBright`.

Source, in order of preference:

1. **Slice the pinned build** (default, no network). The legacy site already
   published `g12.bin` + `manifest.json` built from Gaia DR3 with
   `phot_g_mean_mag <= 12`; this builder slices that blob per tile and checks
   the header count against the manifest before writing anything.

   ```sh
   git cat-file blob origin/legacy/rn-site/main:static/stars/lod/g12.bin > data/legacy/g12.bin
   git cat-file blob origin/legacy/rn-site/main:static/stars/lod/manifest.json > data/legacy/manifest.json
   python3 tools/starcat/build_star_lod.py --from-legacy data/legacy
   python3 tools/starcat/build_star_lod.py --verify-only
   ```

2. **Re-query the archive** when the pinned release is refreshed —
   `python3 tools/starcat/build_star_lod.py --from-tap`. Per tile, against
   `https://gea.esac.esa.int/tap-server/tap/sync` (ADQL, CSV, 4 in flight,
   resumable under `data/tap/lod/`):

   ```sql
   SELECT source_id, ra, dec, phot_g_mean_mag, bp_rp
   FROM gaiadr3.gaia_source
   WHERE phot_g_mean_mag <= 12
     AND ra >= :ra_lo AND ra < :ra_hi AND dec >= :dec_lo AND dec < :dec_hi
   ```

Licence: ESA/Gaia/DPAC, freely redistributable with attribution.

## build_detail.py — one JSON payload per star

Stages run in order and each caches under `data/tap/`, so an interrupted build
resumes rather than re-pulling. `--stage <name>` runs one.

```sh
python3 tools/starcat/build_detail.py                 # all stages
python3 tools/starcat/build_detail.py --stage gaia    # just the long pull
python3 tools/starcat/build_detail.py --stage selftest
```

### gaia — Gaia DR3, 768 tiles, `data/tap/gaia/`

`https://gea.esac.esa.int/tap-server/tap/sync`, ADQL, CSV, ≤ 4 concurrent,
exponential backoff on 429 and 5xx. The join is a LEFT OUTER JOIN so stars with
no astrophysical row survive.

```sql
SELECT g.source_id, g.ra, g.dec, g.parallax, g.parallax_error, g.pmra, g.pmdec,
       g.radial_velocity, g.ruwe, g.phot_g_mean_mag, g.phot_bp_mean_mag,
       g.phot_rp_mean_mag, g.bp_rp, g.phot_variable_flag, g.non_single_star,
       a.teff_gspphot, a.distance_gspphot, a.radius_flame, a.lum_flame,
       a.mass_flame, a.age_flame
FROM gaiadr3.gaia_source AS g
LEFT OUTER JOIN gaiadr3.astrophysical_parameters AS a ON g.source_id = a.source_id
WHERE g.phot_g_mean_mag <= 12
  AND g.ra >= :ra_lo AND g.ra < :ra_hi AND g.dec >= :dec_lo AND g.dec < :dec_hi
```

Licence: ESA/Gaia/DPAC — <https://gea.esac.esa.int/archive/>.

### hyg — HYG v3

`https://raw.githubusercontent.com/astronexus/HYG-Database/main/hyg/v3/hyg_v38.csv.gz`,
gunzipped into `data/tap/hyg_v3.csv`. Kept fields: `hip`, `hd`, `hr`, `gl`,
`bf`, `bayer`, `flam`, `proper`, `spect`, `con`, `mag`, `absmag`, `ci`, `lum`,
`dist`, `ra`, `dec`.

HYG carries no Gaia cross-id, so the Gaia join is **positional**: the Gaia
position is walked back from epoch 2016.0 to J2000 using its own proper motion,
then matched to the nearest HYG row within 3″ whose V magnitude is within 2 of
Gaia G. Spot-checked against σ Octantis, which resolves to the right HYG row.

Licence: CC BY-SA 4.0, Astronomy Nexus (David Nash).

### iau — IAU WGSN Catalog of Star Names

`https://www.pas.rochester.edu/~emamajek/WGSN/IAU-CSN.txt` → `data/tap/iau_csn.txt`.
451 approved names; the fixed-column rows are parsed for name, diacritic form,
designation, Bayer letter, constellation, V magnitude, HIP, HD, RA/Dec. Joined
on HIP first, HD second.

Licence: IAU, CC BY 4.0 — <https://www.iau.org/public/themes/naming_stars/>.

### bounds — IAU constellation boundaries

`https://cdsarc.cds.unistra.fr/ftp/VI/42/data.dat` (VizieR VI/42, Roman 1987;
Delporte's 1930 boundaries) → 357 arcs as `RAl,RAu,DEl,cst` in
`data/tap/constellation_boundaries.csv`. Lookup precesses the ICRS position to
B1875 (the epoch the boundaries are drawn in) and takes the first arc whose
declination and RA range contain it.

`--stage selftest` asserts Sirius → CMa, Vega → Lyr, Polaris → UMi,
Betelgeuse → Ori, and the build refuses to continue if any fails.

Licence: CDS/VizieR, free with attribution.

### simbad — bright catalog only

`https://simbad.cds.unistra.fr/simbad/sim-tap/sync`, batches of ≤ 500
identifiers, cached under `data/tap/simbad/`. Queried for the ~12.2k ids in
`tools/starcat/data/gaia_g6.5.csv` (`'Gaia DR3 <source_id>'`) and
`hip_2.5.csv` (`'HIP <n>'`) — the stars the panel can actually pick.

```sql
SELECT i.id AS query_id, b.main_id, b.otype_txt, b.sp_type, b.plx_value,
       b.pmra, b.pmdec, b.rvz_radvel, a.id AS other_id
FROM ident AS i
JOIN basic AS b ON b.oid = i.oidref
JOIN ident AS a ON a.oidref = i.oidref
WHERE i.id IN (...)
```

Licence: SIMBAD, CDS Strasbourg — cite 2000,A&AS,143,9 (Wenger et al.).

### merge — the shipped file

Writes `data/sky_star_detail.csv.zst` (gzip if `zstd` is not on PATH), two
columns: `id` (`g<gaia source_id>` or `h<hip>`) and a JSON payload. Keys with
no value are dropped, so a payload only carries what is actually known. Every
payload has a `sources` array naming the catalogues it drew on, and, where the
position is known, `constellation` + `constellationName`.

Alongside it: `data/sky_star_detail.sha256` and `data/sky_star_detail.stats.json`
(row counts per source and join hit rates). Both are committed; the dataset
itself — 246 MB compressed, 2.1 GB of CSV — is not.

Floats are rounded to six decimals: 3.6 mas on a position, and past the
uncertainty on everything else here. Carrying full float64 repr instead costs
another 80 MB for no information.

## Refresh

```sh
python3 tools/starcat/build_star_lod.py                # tiles (no network)
python3 tools/starcat/build_detail.py                  # ~40 min, resumable
python3 tools/starcat/load_detail.py --run             # into $DATABASE_URL
```

Delete the matching directory under `data/tap/` to force a stage to re-pull.

## Loading

`sky_star_detail` is created by the drizzle migration, **not** by this loader.
Expected schema:

```sql
sky_star_detail(
  id       text        primary key,
  payload  jsonb       not null,
  built_at timestamptz not null
)
```

`load_detail.py` emits `load_detail.sql`, which refuses to run when the table is
missing and otherwise, in one transaction, `\copy`s the CSV into a temp table
(from `PSTDIN` — inside a `-f` script plain `STDIN` would read the script file
itself), truncates `sky_star_detail` and inserts the lot. A build is the whole
catalogue, so it replaces rather than merges. Nothing is written to disk in
between:

```sh
pnpm db:load-sky        # zstd -dc … | psql "$DATABASE_URL" -f tools/starcat/load_detail.sql
```

Run it once per environment after `pnpm db:migrate`. Roughly 5 minutes for the
3.09M rows; the table lands at about 2.5 GB including its primary key.
