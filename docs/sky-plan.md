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
