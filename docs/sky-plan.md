# Sky ladder S1–S4: quality, depth, streaming, detail

Owner 2026-09-07: stars must look high quality; more stars must stream in,
nearest the view centre first and ahead along the orbit; lookups may use the
server to keep transfer small; clicking a star shows substantial data.

Prior art consulted: Gaia Sky star rendering (pseudo-size from magnitude,
billboards over GL_POINTS), Stellarium `StelSkyDrawer` (radius + luminance
from magnitude, eye adaptation, limit magnitude), Celestia #1948 (linear
brightness `10^(-0.4 m)·exposure`, photopic eye PSF, glare diameter bound),
Spencer et al. 1995 glare PSF (`0.384 f0 + 0.478 f1 + 0.138 f2`), "Smaller
than Pixels" (Weimar 2025: energy-conserving sub-pixel splatting into an HDR
buffer, then tone map), HiPS (HEALPix tiles by order for progressive
catalogues), SIMBAD TAP (`ident` → `basic`, `allfluxes`), Gaia DR3
`astrophysical_parameters` (teff, distance, radius, luminosity, mass).

## Code organisation

`src/features/sky/` stays the owner. New: `stars-gl.ts` (GL point pass),
`lod.ts` (tile index + priority queue), `pick.ts` (nearest-star pick),
`detail.tsx` (panel), `src/app/api/sky/star/[id]/route.ts` (detail read), `tools/starcat/` (Python builders carried from legacy/universe),
`drizzle/0006_sky_star_detail.sql`. Caps: no file over 400 lines; the frame
loop stays in `stage.ts`.

## S1 — stars in WebGL2, physically scaled (deploy after)

- Catalog `STR2`: 28-byte records = STR1 + `u64 id` + `u8 kind` (0 Gaia DR3,
  1 HIP) + 3 pad; rebuilt by `tools/starcat/build_bright.py` from the
  committed `gaia_g6.5.csv` + `hip_2.5.csv` (ids retained). Colour from a
  blackbody ramp via BP−RP → Teff (Casagrande-style polynomial), 24 levels.
- Render: one `GL_POINTS` pass with a per-fragment profile, additive into a
  float RGBA16F framebuffer, then a tone-map pass to the screen.
  - linear brightness `b = 10^(-0.4(m − m_ref))` × extinction(airmass), `m_ref` per theme.
  - profile: core Gaussian σ≈0.6 px (energy-conserving: sub-pixel stars sum,
    never disappear) + glare term `b^0.5 · k/(d+ε)^2` bounded at 24 px; no discard rim.
  - point size from `b` with a hard cap (Celestia's ~1° rule → 24 px here).
  - tone map: `1 − exp(−x·exposure)` per channel, so cores saturate to white
    while wings keep the blackbody colour; light theme raises `m_ref` and lowers exposure.
  - fallback: current canvas path stays for no-WebGL2.
- Gate: screenshots at 1×/2× DPR, dark/light, phone; Sirius/Vega vs a mag-5
  star side by side; no visible tiers or colour banding; 30 fps with 12k stars
  (and 200k in the S2 rehearsal).

## S2 — depth: streamed Gaia G≤12, view-first, orbit-ahead

- Assets: `g9.bin` (G≤9, 2.8 MB, whole file, fetched after first paint) and
  768 tile files `public/stars/lod/<id>.bin` (G≤12, 16-byte records, ~64 KB
  each) split from legacy `g12.bin` by `build_star_lod.py`; served static,
  immutable cache headers, no server logic.
- Priority (client, `lod.ts`): each tick compute the zenith direction and the
  frame's angular radius; score tile = distance(tile centre, zenith) − bonus if
  inside frame; then add tiles the orbit will centre in +1, +2, +3 sim-minutes
  (observerAt(sim + Δ)) at lower priority; fetch ≤2 in flight, ≤N tiles
  resident (LRU by last-needed), abort on view jump (globe drag).
- Rendering: g9 as a second GL buffer at the S1 profile; g12 tiles drawn only
  as sub-pixel flux (they read as sky texture, not points) and skipped on
  `saveData`/coarse devices; magnitude limit slides with DPR.
- Dedupe: g9/g12 records with an id already in STR2 are dropped at build.
- Gate: waterfall shows centre tiles first then the orbit-ahead ones; total
  transfer in 5 min ≤ 6 MB; frame time unchanged.

## S3 — hover + pick any star, detail panel with server lookup (deploy after)

- Pick (`pick.ts`): per frame the GL pass already has screen positions; a
  pointer query finds the nearest star within 14 px weighted by brightness,
  over STR2 then g9 (g12 not pickable). Hover: ring + tag (name if named, else
  `Gaia DR3 …`/`HIP …`, magnitude, colour). Click pins the star as a callout;
  Escape/click-empty unpins. Coarse pointers: tap = pick.
- Datasets are LOCAL (owner 2026-09-07): the browser and the server never
  contact ESA or CDS at runtime. `tools/starcat/build_detail.py` pulls once,
  offline, per sky tile: Gaia DR3 G≤12 `gaia_source` + `astrophysical_parameters`
  (parallax, pmra/pmdec, radial_velocity, ruwe, phot_variable_flag,
  non_single_star, G/BP/RP, teff/distance/radius/lum/mass), HYG v3 (HIP, HD,
  Bayer, Flamsteed, proper names, spectral type; CC BY-SA), IAU star names,
  and one bulk SIMBAD TAP pull for the 12,191 STR2 stars (main_id, otype_txt,
  sp_type, all ids). Output `data/sky_star_detail.csv.zst` (gitignored,
  ~100 MB) + `data/sky_star_detail.sha256` (committed). Loader
  `pnpm db:load-sky` COPYs into `sky_star_detail(id text pk, payload jsonb)`;
  run once per environment after migrate 0006; the deploy recipe gains that step.
- Detail route `GET /api/sky/star/<kind>-<id>` reads Postgres only; missing row
  → catalog-only payload (never a 500). Constellation from IAU boundaries
  (VizieR VI/42) computed at build and stored in the row.
- Panel (`detail.tsx`): names (IAU/Bayer/Flamsteed/HD/HIP/Gaia), constellation
  (IAU boundary lookup, VizieR VI/42, packed offline), object type, spectral
  type, temperature, distance (parallax + Gaia distance), luminosity, radius,
  mass, proper motion, radial velocity, magnitudes (V, B, G), plus links to
  SIMBAD and the Gaia archive. Numbers in a table; empty rows omitted.
- Gate: pick Alioth by hover, click, panel shows SIMBAD row (`* eps UMa`,
  A1III-IVpkB9); pick an unnamed mag-6 star → Gaia-only panel; a DB row exists
  after the first fetch; second fetch hits no upstream.

## S4 — polish

Milky Way re-bake at HEALPix level 9 (NSIDE 512, ~0.11°) into a 4096×2048
map (`tools/starcat/mwcat.py` carried from universe; the 1024×512 map reads
as blotches when the galactic centre fills the frame at 2× DPR).
Constellation lines toggle (Stellarium `constellationship.fab`, packed);
twinkle off by default (reduced-motion respected); named-star list grows to
the IAU full set (~450) since ids now resolve names server-side.

## Rules

Plan rules apply (PUBLIC_ORIGIN, audit-free read paths, screenshots viewed
both themes + phone before any gate). The browser never contacts ESA or CDS
directly; only `/api/sky/*` does. Assets carry NOTICE updates.

## S1 deviations

Recorded as built, 2026-09-07.

- **The STR2 record is 32 bytes, not 28.** The field list this section gives —
  20 bytes of STR1, a `u64 id`, a `u8 kind`, 3 pad — sums to 32; 28 was an
  arithmetic slip. The fields are exactly as specified.
- **The colour relations are named ones, checked against their papers.**
  BP−RP → Teff is Mucciarelli & Bellazzini 2020 (RNAAS 4, 52) at solar
  metallicity, not a Casagrande fit; B−V → Teff for the Hipparcos rows is
  Ballesteros 2012 (EPL 97, 34008). The 24 levels are spaced evenly in 1/T over
  2900–17000 K, which is the range the shipped catalogue actually occupies —
  a wider ramp would spend levels on stars that are not in the file.
- **The glare wing grows as `b^0.75`, not `b^0.5`.** The square root was built
  first and made every star from about magnitude 6 up wear the same halo: it
  compresses a thousand-to-one flux range into thirty-to-one. `b^0.75` puts
  the drawn radius on `b^0.375`, which is the ten-to-one range of widths the
  sky shows.
- **Width is floored below about magnitude 4.5**, where the glare wing is
  narrower than the core. Those stars are a point of light and nothing else,
  and a smaller quad would clip the Gaussian.
- **The highlight ring moved into GL.** There is no 2D canvas on the shipped
  path to draw it on, so it is a 64-segment `LINE_LOOP` generated from
  `gl_VertexID` after the tone map.
- **`preserveDrawingBuffer` is gated on `?skyreadback=1`.** A Playwright
  `evaluate` runs between frames, so without it `readPixels` sees zeros; the
  alternative was reading back from the page's own loop, which would put
  test-only code in the render path.
- **The end-to-end assertion is about the brightest first-magnitude star in
  frame, not Sirius by name.** The observer flies a fixed orbit and which of
  Sirius, Vega, Arcturus and the rest is overhead is not the spec's to choose;
  it takes the brightest that is, and Sirius when Sirius is it. Its comparison
  magnitude-5 star is picked at the same altitude, so the atmosphere's share
  is the same for both and the magnitude difference is what is being measured.

## S2 deviations

Recorded as built, 2026-09-07.

- **Four modules, not one.** `lod.ts` is the file format, the colour ramp and
  the magnitude cut; `lod-tiles.ts` the grid and the priority queue;
  `lod-stream.ts` the fetching, the budget and the LRU; `deep-gl.ts` the two
  GPU buffers; `deep-stage.ts` the half-second tick and the debug readout. One
  file would have been eight hundred lines, and the queue is the piece worth
  reading on its own.
- **Tiles are scored from their near edge, not their centre.** On a 32x24
  equal-angle grid a polar cell is a sliver and an equatorial one is seven
  degrees across, so centre distance ranks a tile that adds nothing to the
  drawable region above one that completes it. `distance` in the queue is
  `angle(zenith, centre) - tileRadius`; the frame bonus and the orbit-ahead
  tail are exactly as specified.
- **The visible region is a coverage _field_, not a cone.** The plan's tile
  fade plus a magnitude ramp inside a tile's edge cannot avoid seams: a taper
  that reaches zero at the boundary makes a dark line between two loaded
  tiles, and one that reaches a half makes a step at the edge of a lone tile.
  What ships is a 32x24 texture holding how much of each tile is resident,
  sampled bilinearly in the vertex shader and eroded by `2(c - 0.5)`. That is
  1 wherever a tile's neighbours have landed, 0 on the boundary of the loaded
  region, and smooth across the outermost tile between them — so there is no
  edge to see whatever has streamed. It subsumes the per-tile 600 ms fade
  (the field eases per frame, on arrival _and_ on eviction) and lets the whole
  tile buffer draw in one call.
- **`Range` requests are used after all, and the tile files were re-sorted to
  make them honest.** A tile on the galactic plane holds 20,278 stars in
  324 KB and the GPU keeps 4,096 of them; paying five times over for the
  discard spent the entire byte budget on six tiles. The files are now written
  brightest-first (`build_star_lod.py`, `--resort` for a build made before the
  ordering was a contract, `sortedByMagnitude` in the manifest), so the front
  of a file is a magnitude cut — spatially even, which a prefix of the old
  source-id order was not — and the client asks for at most
  `12 + 4096 x 16` bytes of any tile. Same records, same counts, same file
  sizes; only the order inside each file changed.
- **The first minute costs 5.0 MB, not the 4 the brief asked for.** The page's
  own assets are 1.47 MB and `g9.bin` is 2.65 MB, so 4.1 MB is spent before a
  tile is asked for: the 4 MB figure was unreachable without cutting g9 or the
  globe's textures. The streamer's own budget — a 640 KB burst then 5 KB/s —
  is set by the plan's "6 MB in five minutes", which it meets: 2.1 MB of tiles
  by five minutes, 6.2 MB in total.
- **Frame timing is reported as the main thread's cost and the frame
  interval.** `gl.finish()` does not stall on this driver, so a
  wall-clock bracket around the draw measures issuing it, not drawing it. The
  readout carries both that and the interval between painted frames, which is
  what a visitor actually sees.
- **The S2 end-to-end lives in `e2e/sky-deep.spec.ts`.** `sky.spec.ts` was
  already 326 lines and the cap is 400.
- **`?skyfill=1` lifts the byte budget**, alongside `?skydebug=1`, so the
  performance gate can fill every slot in a minute rather than a quarter of an
  hour. Neither flag is reachable without typing it.

## S3 deviations

Recorded as built, 2026-09-07.

- **The dataset's ids and the URL's ids are different spellings, and the route
  is where they meet.** `sky_star_detail.id` stays as the CSV carries it,
  `g<source_id>` / `h<hip>`; the browser asks with `starKey()`'s `gaia-<n>` /
  `hip-<n>`. Rewriting three million keys through a COPY to match a URL is work
  done three million times to save one line.
- **A `hip-` key is looked up twice, and migration 0007 is why.** The bright
  catalogue calls a first-magnitude star by its Hipparcos number and the detail
  build filed it under its *Gaia* source id wherever the positional match
  landed: Alioth is `hip-62956` to the sky and `g1576683529448755328` to the
  table, with `hip: "62956"` in the payload. Only the 66 Hipparcos stars Gaia
  has no row for are keyed `h<n>`. Without the second lookup — the primary key,
  then `payload->>'hip'` over the index 0007 adds — every bright named star a
  visitor clicks answers "no further record" about the best-known stars in the
  sky. The visual gate found this; every test passed while it was true.
- **HYG's constellation beats the boundary walk, and HYG's `proper` beats the
  IAU name fields.** The build derives a constellation by precessing to B1875
  and walking the VI/42 arcs; it puts Rigil Kentaurus in Circinus, Mimosa in
  Centaurus and Fomalhaut in Sculptor. HYG names one per star, curated, and is
  right in every case checked; the two disagree on 13% of the stars HYG knows.
  Separately, the IAU name list is parsed out of fixed columns, so a two-word
  name lands split — HIP 71683 arrives as `iauName` "Rigil" and
  `iauNameDiacritics` "Kentaurus", and neither half is the star's name.
  `constellations.ts` carries the 88 abbreviation/name pairs the panel needs,
  and the lookup itself is what rejects HYG's two malformed codes.
- **Fourteen pixels is the reach for every star; brightness decides who wins
  inside it.** The plan says "nearest within 14 px weighted by brightness". A
  star's own drawn half-width (`tuning.ts` `starDiameterPx`) comes off the
  distance, so a pointer inside Sirius's disc cannot be stolen by a
  magnitude-8 neighbour three pixels nearer — but brightness never *extends*
  the reach, or a first-magnitude star would swallow twenty pixels of sky.
- **`g9.bin` keeps its source ids after all.** S2 dropped them because nothing
  could then ask which star; picking is what asks. 165,393 ids is 1.3 MB. The
  768 streamed tiles still drop theirs — they are not pickable, and three
  million ids nobody reads would be 25 MB of nothing.
- **Three modules came out of `stage.ts`, which was at its 400-line cap.**
  `globe-stage.ts` (the mini-globe), `stage-assets.ts` (the order the assets
  arrive in) and `pick-stage.ts` (which star is hovered, which is pinned, where
  either is on screen a frame later). The interaction is a fourth file,
  `star-pick.tsx`, beside the panel's `detail.tsx`: the pointer wiring and the
  hover tag are not the panel.
- **The label separation is measured against the label now.** 328 px of a
  1440 px frame is 0.46 in normalised coordinates and the constant was 0.55; on
  a 390 px phone the same label is 1.68 across, so two callouts a third of the
  width apart passed the test and were printed on top of each other. The panel
  made it visible by taking the bottom 62% of a phone, but the defect was
  S1's and is fixed there. Desktop is unchanged.
- **`window.__sky` grew a `named` array**, so the end-to-end can drive the
  pointer at whichever star is actually up rather than at a star the orbit may
  have set. It is still `?skydebug=1` only, and the install moved from
  `deep-stage.ts` to `stage.ts` because the stage is what knows what is on
  screen.
- **The panel says a temperature from BP−RP where Gaia has no astrophysical
  row**, labelled as that rather than as `teff_gspphot`. Alioth has no
  astrophysical row and a colour temperature is worth more than a blank.
