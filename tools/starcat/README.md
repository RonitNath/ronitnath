# starcat — the offline sky datasets

Everything the sky draws or shows is pulled here, once, and shipped. The browser
and the API never contact ESA, CDS or GitHub at runtime. Python 3 standard
library only; no virtualenv, no pip.

Bulk artifacts live under `data/`, which is gitignored. The LOD tiles under
`public/stars/lod/` are the exception: they are the shipped assets and are
committed.

| Script | Builds | Committed output |
| --- | --- | --- |
| `build_bright.py` | the naked-eye catalogue | `public/stars/bright-<hash>.bin`, `data/hip_filled.json` |
| `mwcat.py` | S4 Milky Way band | `public/sky/milkyway*.webp`, `source.json` |
| `build_lines.py` | S4 constellation figures | `public/sky/lines-<hash>.bin`, `lines.json` |
| `build_names.py` | S4 named stars | `public/stars/named-<hash>.json` |
| `build_star_lod.py` | S2 streamed depth | `public/stars/lod/<id>-<hash>.bin`, `manifest.json` |
| `name_assets.py` | the content hashes | renames, `src/features/sky/asset-names.ts` |
| `build_detail.py` | S3 star detail | `data/sky_star_detail.sha256`, `.stats.json` |
| `build_delta.py` | detail for a catalogue fill | `data/sky_star_detail.delta.csv` |
| `load_detail.py` | the psql loader | `load_detail.sql` |

Every asset the browser fetches is named `<stem>-<sha256[:12]><suffix>`, so it
can be served `immutable` for a year and a rebuild never has to be purged.
`build_star_lod.py` names the tiles from the sha256 its manifest already
records; `name_assets.py` stamps the rest and writes the map the client reads.
**Run it last**, after every builder that writes into `public/`. Builders read
through `name_assets.current()` rather than a literal path, because the file
they wrote last time is not where they wrote it.

`pnpm db:load-sky` runs `scripts/load-sky.mjs`, not `load_detail.py`: the
deployment's image is `node:24-alpine` with the application's own dependencies
and no Postgres client, so the shipped loader streams the file through Node's
own zstd decoder and inserts in batches through `pg`. Same transaction, same
full replacement, and about 80 seconds rather than five minutes.
`load_detail.py` remains for a psql-driven load where `zstd` and `psql` are
both to hand.

`mwcat.py`, `healpix.py`, `build_lines.py` and `build_names.py` need numpy,
Pillow and scipy for the bake; the rest is standard library. `iau_csn.py` is
shared by `build_names.py` and `build_detail.py`.

## mwcat.py — the Milky Way band

36.8 million Gaia sources brighter than G 15, counted into HEALPix level 9
(NSIDE 512, 3,145,728 cells of 0.115°) and resampled into a 4096x2048
equirectangular map in equatorial J2000 — the projection `band-gl.ts` samples.
Counts, not summed flux: summed flux is dominated by whichever single star in a
cell is brightest, which bakes as salt-and-pepper. The gold cast is the
flux-weighted BP-RP colour, which really does run redder in the plane.

Gaia's `source_id` carries its level-12 NESTED HEALPix index in the high bits,
so `source_id / 2^41` is the level-9 index with no coordinate maths. The whole
sky is one aggregation, asked as 192 contiguous `source_id` ranges: the archive
runs the whole-sky version but resets the result stream after a megabyte or
two, with no `Range` to resume from. Each chunk caches under `data/tap/mw/`.

```sh
python3 tools/starcat/mwcat.py --selftest   # HEALPix equal-area round trip
python3 tools/starcat/mwcat.py              # ~10 min of pulls, then the bake
```

The output name carries the content hash — `milkyway-<12 hex>.webp` plus a
`milkyway-2k-<hash>.webp` downscale — and `assets.ts` names both. A re-bake is
therefore a new URL: nothing has to be purged from a CDN or a visitor's cache.
Update `ASSETS` when you re-bake. `public/sky/source.json` records the query,
the row count, the sizes and the checksums.

Licence: ESA/Gaia/DPAC, CC BY-SA 3.0 IGO.

## build_lines.py — the constellation figures

Stellarium's `skycultures/modern/constellationship.fab` (pinned at v23.4; the
file moved after that release) draws each of the 88 figures as Hipparcos pairs.
The browser has no Hipparcos catalogue, so the join happens here: HYG v3 gives
each HIP a J2000 position, and the position resolves to the nearest record in
`bright.bin` within 90 arcseconds whose magnitude agrees, or within 60 with no
magnitude test where the star is a large-amplitude variable and HYG's single
number is wrong for it.

```sh
python3 tools/starcat/build_lines.py --report
```

674 of Stellarium's 676 segments survive. One HIP number does not resolve —
HIP 33165 is V 6.65, past the catalogue's own limit — and a segment that does
not resolve at both ends is refused rather than guessed, which costs Canis
Major two of its seventeen. `public/sky/lines.json` records the counts, the
unresolved numbers and the content hash of the catalogue the indices address.

Licence: CC BY-SA 4.0 + Free Art License (`skycultures/modern/info.ini`).

## build_names.py — the named stars

Every name in the IAU Catalog of Star Names whose star is in `bright.bin`: 337
of the 451, the difference being the ~110 fainter than magnitude 6.5, plus
Regor and R Doradus, which the IAU list does not carry.
Position-matched within two arcminutes, with brightness breaking a tie —
Alpha Centauri's two components are five arcseconds apart and the pair moves
3.7 arcseconds a year, so both records are within half an arcminute of the IAU
position and the nearer one is the fainter component.

Constellation is the IAU row's own three-letter code expanded through
`src/features/sky/constellations.ts`, read from that module so the two cannot
drift. Classification is HYG's spectral type as a phrase (`A1V` is "A-type
main-sequence star"; `K1.5III` is "orange giant"). Distance is HYG's parallax
distance in light years.

The fifty entries S1 wrote by hand live at `data/named_hand.json` and keep
their text; that file is the build's *input*, never its output, or a second run
would read a generated phrase as a person's reading. The four the IAU list does
not carry resolve through their own `hip` and HYG's position — never through a
`brightIndex`, which is written against the catalogue of the day and means a
different star after any rebuild.

`named.json` records the content hash of the catalogue it resolved against, as
`lines.json` does. Every record `build_bright.py` adds moves the indices after
it, so a stale name list labels the wrong stars plausibly; the hashes are what
let a test refuse the pair.

```sh
python3 tools/starcat/build_names.py --dry-run
python3 tools/starcat/build_names.py
```

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
the Hipparcos numbers the catalogue actually carries (`'HIP <n>'`): the
saturated bright end of `hip_6.5.csv` plus everything in `hip_filled.json`.
Those are the stars the panel can be asked about under an `h<hip>` key.

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

## build_bright.py — the naked-eye catalogue

`gaia_g6.5.csv` (Gaia DR3, G < 6.5) and `hip_6.5.csv` (Hipparcos New Reduction,
Hp < 6.5, with `pm_ra`/`pm_de`), merged in three tiers. Every Hipparcos row is
first carried forward 24.75 years by its own proper motion, from that
catalogue's epoch of 1991.25 to Gaia's of 2016.0; without that ε Cygni lands
twelve arcseconds from its own Gaia record and is added a second time.

- **Hp < 2.5** — Gaia is saturated and unusable, so Hipparcos is authoritative
  and the Gaia row within 3 arcsec and 1.5 mag of it is dropped. 89 stars.
- **Below that** — Gaia is authoritative, and a Hipparcos row is *added* only
  where no Gaia row is within 3 arcsec and 1.5 mag. 198 stars, five of them
  second magnitude (Sheratan, Menkar, Mahasim, Gienah, Enif) that the old
  Hp < 2.5 cut passed just above and Gaia records only as saturated pixels.
- Where a star is added, every Gaia record within 3 arcsec of it goes too,
  magnitude or no magnitude: 47 of the 198 have such a ghost, and left alone it
  draws the star twice at two magnitudes.

12,335 records. The added stars are written to `data/hip_filled.json`, which is
what `build_star_lod.py` clears a radius around and `build_delta.py` builds
detail rows for.

```sh
python3 tools/starcat/build_bright.py
```

## build_delta.py — detail rows for a fill

`sky_star_detail` is 3.09M rows and 257 MB compressed; rebuilding it to answer
for two hundred new stars is a day of TAP for a change of 0.006%. This asks
`build_detail.py`'s own routines about exactly the numbers in
`hip_filled.json` — HYG, the IAU list, the boundary walk, one small SIMBAD
batch cached under `data/tap/simbad-delta/` — and writes
`data/sky_star_detail.delta.csv`. Load it with
`node scripts/load-sky.mjs --delta <file>`, which upserts instead of replacing.

## Refresh

```sh
python3 tools/starcat/build_bright.py                  # seconds
python3 tools/starcat/build_names.py                   # seconds
python3 tools/starcat/build_lines.py                   # seconds
python3 tools/starcat/build_star_lod.py --resort       # tiles (no network)
python3 tools/starcat/mwcat.py                         # ~10 min, resumable
python3 tools/starcat/name_assets.py                   # last: the hashes
python3 tools/starcat/build_delta.py                   # if the catalogue grew
python3 tools/starcat/build_detail.py                  # ~40 min, resumable
pnpm db:load-sky                                       # into $DATABASE_URL
```

The order matters: `build_names.py` and `build_lines.py` resolve indices into
whatever `build_bright.py` last wrote, and `name_assets.py` cannot stamp a file
that has not been rebuilt yet.

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
