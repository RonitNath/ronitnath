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
`detail.tsx` (panel), `src/app/api/sky/star/[id]/route.ts` (detail lookup +
cache), `tools/starcat/` (Python builders carried from legacy/universe),
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
- Detail route `GET /api/sky/star/<kind>-<id>`: server queries SIMBAD TAP
  (`ident`→`basic`+`allfluxes`+`ids`) and Gaia TAP (`gaia_source` +
  `astrophysical_parameters`), normalises to one JSON, stores in
  `sky_star_detail(id, payload jsonb, fetched_at)`; served from the table when
  present, refreshed after 180 days; upstream failures return the catalog-only
  view (never a 500). Rate-limited per client; 8 s upstream timeout.
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
