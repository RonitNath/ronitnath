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

## S4 deviations

Recorded as built, 2026-09-08.

- **The whole-sky query is asked 192 times, not once.** The archive will *run*
  a level-9 aggregation over `gaia_source` — it is one indexed pass — but it
  will not deliver it: the async job's result stream for three million rows is
  reset by the front end after a megabyte or two, and there is no `Range`
  support to resume from. Splitting the query along the HEALPix index gives 192
  contiguous `source_id` ranges the server answers without a scan, each about
  16k rows and half a megabyte, and each arrives whole. `public/sky/source.json`
  carries the query with its range placeholders and the chunk count. The pull
  is 3,065,685 rows over 3,145,728 pixels — the 80,043 empty ones are Poisson,
  not a gap: at level 9 a high-latitude pixel holds a handful of stars.
- **The band map is 4096x2048 at 315 KB**, under the 600 KB the brief allowed,
  with a 2048x1024 downscale at 142 KB for anything under 1,600 device pixels
  across. The angular blur is 0.16° rather than the legacy 0.45°: at level 7
  the blur was hiding a 0.46° lattice, and at level 9 the same blur would hide
  the dust instead. `flux` is dropped from the SELECT — the baker never read it,
  and it was a third of the bytes.
- **The map's filename carries its content hash.** `milkyway-<12 hex>.webp`,
  written by the baker and named in `assets.ts`. A re-bake at a fixed path is
  invisible behind a CDN and to everyone holding the old one, and the
  alternative was a Cloudflare purge on every bake. Nothing has to be purged.
- **Mipmaps needed `textureGrad`, and finding out why was the visual gate's
  work.** Turning on `LINEAR_MIPMAP_LINEAR` drew a dashed dark curve across the
  band. The map wraps in u, and the hardware picks a level from the screen-space
  derivative of the coordinate it is handed: across the seam at RA 180 that
  derivative jumps by a whole turn for the one 2x2 quad straddling it, which
  selects the coarsest mip for that quad. Wrapping `dFdx`/`dFdy` back into
  [-0.5, 0.5] and sampling with them explicitly is the fix.
- **The figures are CC BY-SA 4.0 + Free Art License, not GPL.** Stellarium's
  `skycultures/modern/info.ini` says so; the file moved out of `master` after
  v23.4, so the pinned source is that tag. The astronexus alternative the brief
  offered does not exist — HYG publishes no line set — so no substitution was
  needed. HYG *is* used, for the HIP positions the join runs through.
- **665 segments of Stellarium's 676, and six Hipparcos numbers unresolved.**
  Sheratan, Menkar, Mahasim, Gienah and Enif are all V 2.5 to 2.7 — Gaia
  saturates on them and `hip_2.5.csv` cuts just above them, so they are not in
  `bright.bin` at all; HIP 33165 is V 6.65, past the catalogue. Eleven segments
  are dropped rather than guessed, which leaves a gap in Aries, Cetus, Auriga,
  Corvus and Pegasus. Closing it means rebuilding `bright.bin` from a deeper
  Hipparcos cut, which moves every record index in the catalogue and is not
  S4's to do.
- **The pass draws quads, not `LINES`.** `lineWidth` is 1 device pixel and
  nothing else on every driver that matters, which at 2x DPR is half a CSS
  pixel of unfeathered diagonal — a dotted line over a star field. Each segment
  is two triangles carrying both endpoints, so the vertex shader works out the
  screen direction and offsets its own corner; the fragment feathers the last
  device pixel. One CSS pixel, antialiased, on a canvas with no MSAA.
- **333 names, not the ~450 the plan guessed.** Only 339 of the IAU's 451
  approved names are brighter than the catalogue's G ≤ 6.5 cut, and seven of
  those are the saturated stars above. The two-word column bug is fixed in a
  shared `iau_csn.py` that reads the file's own fixed columns — and reading
  *those* found a second bug the regex shared: Mebsuta leaves the component
  column blank, so a whitespace split loses a field and shifts every column
  after it. `build_detail.py` now reads through the same module.
- **Alpha Centauri needed a magnitude tiebreak.** Its two components are five
  arcseconds apart and the pair moves 3.7 arcseconds a year, so at the epoch
  difference between the IAU file and the catalogue both records sit within half
  an arcminute of the position and the *nearer* one is the fainter component.
  Candidates are bucketed by separation to two arcminutes and brightness decides
  inside the bucket, which puts Rigil Kentaurus on A and Toliman on B.
- **The fifty hand-written entries moved to `tools/starcat/data/named_hand.json`.**
  They were the build's input and its output at the same file, so a second run
  would have read last night's generated phrase as a person's reading and the
  distinction would have been gone. Two of the fifty are superseded by their
  star's IAU name (Alpha Centauri by Rigil Kentaurus, Delta Velorum by
  Alsephina); the rest keep their text.
- **Growing the list to 333 exposed a missing factor of two in the callout
  separation.** A label hangs off its anchor by a label width plus the leader
  offset, so two anchors whose leaders turn toward each other need *twice* that
  between them; `separationFor` measured one. With fifty candidates two landing
  near each other was rare, and with 333 it is the common case — the visual gate
  found Polaris and Deneb printed over each other 403 px apart on a 1440 px
  frame. Both axes now come from `reach`, which already said exactly this.
- **Twinkle is `?twinkle=1` and not a button.** Three controls fit the corner on
  a desktop and wrap to two rows on a 390 px phone, which puts a second row of
  chrome under the grounding caption. Two controls and a query parameter is the
  quieter answer; the scintillation itself is in the star vertex shader, tied to
  airmass rather than to brightness, magnitude 3.2 and brighter, and never under
  reduced motion.
- **The light theme's hover and pressed states were unreachable.** The dusk
  re-tint `:root[data-theme='light'] .sky-control` is a whole selector more
  specific than `.sky-control:hover`, so in the light theme the button had been
  the same ink whatever it was doing since R1. Said again at that specificity.
- **`stage.ts` gave up `stage-view.ts` to stay under 400 lines.** Where the
  named stars are on screen, what colour a callout's ring is, and which ring
  this frame draws: three questions answered from a view matrix and a catalogue,
  which is not what a stage is for.
- **The S3 detail end-to-end now pauses before it picks.** It drove the pointer
  at coordinates read a few frames earlier, and at 60x with the pick weighing
  brightness against distance that is enough for a magnitude-5 neighbour of
  Arcturus to win. Paused, the frame the positions came from is the frame the
  pointer lands in.
- **A star that already has a callout gets no hover tag.** The tag and the
  callout say the same star's name a few pixels apart, so hovering Arcturus
  printed "ARCTURUS / mag 0.11 · orange" across the callout's second line and
  neither could be read. The callout goes into a hover state instead — the ring
  brightens, the detail line comes up to full ink — which is the same answer
  said once. For every *other* star the tag is now placed the way a callout's
  leader is: four corners of its star in preference order (right-below first,
  because the leaders prefer up-and-out), and the first that lands on no callout
  label, no hero card and no chrome. `tag.ts` is the geometry, judged against
  the boxes the callouts already measured into `dataset.labelWidth/Height`, so
  nothing in the frame loop reads layout.
- **A tag's trespass is counted as area, where a callout's is counted as
  distance.** `leaderPenalty` scores a block by the shorter of its two overlaps,
  because a callout it turns away can be moved and what it wants to know is how
  far. A tag has four corners and no other move, so what it wants to know is
  which corner hides the least text: the area lost over the tag's own height,
  which is the width of text under it. Clipping the tail of a callout's detail
  line by two pixels of height is nearly no distance at all and is the whole end
  of the line, and the shorter-overlap rule chose exactly that.
- **The first-minute byte budget moved from 5.2 MB to 5.4 MB.** The band is
  126 KB more on the run's own viewport, once, for a band that no longer reads
  as blobs at the galactic centre.

## Catalogue fill + content addressing

Owner 2026-09-08: fill the hole in the catalogue, and content-address every
asset the sky fetches so a deploy never invalidates a byte.

### The hole, and why neither half claimed it

S4 recorded five naked-eye stars drawn nowhere at all — Sheratan, Menkar,
Mahasim, Gienah and Enif, all V 2.4 to 2.7 — and eleven constellation segments
dropped for want of them. The cause is the merge S1 shipped. Gaia DR3's
`gaia_source_lite` at G < 6.5 was the catalogue; `hip_2.5.csv`, cut at Hp < 2.5,
was the supplement for the bright end Gaia saturates on. Gaia is unusable above
G ≈ 1.7 and unreliable to about G 6, so the band between the two cuts belonged
to neither: Gaia records those stars as saturated pixels — Mahasim reads G 7.28
for a V 2.62 star, seventy-five times too faint — and Hipparcos was not asked.

The fill pulls the whole naked-eye Hipparcos catalogue (`hip_6.5.csv`,
Hp < 6.5, 7,982 rows, 578 KB) and merges in **three tiers** rather than two.
Brighter than Hp 2.5 Hipparcos is authoritative and the matching Gaia row is
dropped, exactly as before — that tier is unchanged, and it is why Sirius, Vega
and Alioth still carry Hipparcos magnitudes. Below it Gaia is authoritative and
a Hipparcos row is only *added* where no Gaia row lies within 3 arcsec and 1.5
magnitudes. 198 stars arrive on those terms, nine of them brighter than fourth
magnitude: Enif, Gienah, Lesath, Mahasim, Menkar, Sheratan and Rasalgethi by
name, plus HIP 26551 and HIP 92862. Every star Gaia does measure keeps Gaia's
own BP−RP colour; nothing regresses onto the B−V ramp.

12,191 stars become 12,335. `named.json` grows from 333 to 339 and
`lines.bin` from 665 segments to 674 of Stellarium's 676.

### Deviations

- **The cross-match had to become epoch-aware before it could be inverted.**
  Run as the brief describes it — Hipparcos positions as published, matched
  against Gaia at 3 arcsec — the fill adds 1,330 stars, and most of them are
  wrong. Hipparcos is epoch 1991.25 and Gaia DR3 is 2016.0: ε Cygni moves 480
  mas/yr and lands twelve arcseconds from its own Gaia record, four times the
  match radius, so it is "missing" and gets added a second time. Carrying every
  Hipparcos row forward 24.75 years by its own proper motion first — the pull
  gains `pm_ra`, `pm_de` for it — cuts the fill from 1,330 to 198, which is the
  number of stars that were actually absent. The propagated position is also
  what is *stored*, so the Hipparcos records now sit in Gaia's frame rather than
  a quarter-century behind it.
- **A star that is added takes its Gaia ghost with it, magnitude or no
  magnitude.** Forty-seven of the 198 *do* have a Gaia record within three
  arcseconds — a saturated one, which is exactly why the magnitude test refused
  it and the star was filled. Left alone it draws the star twice at two
  magnitudes: HIP 92862 at 3.93 with a ghost at 2.36 two arcseconds away. So
  once the build has decided Gaia has no usable row for a star, every Gaia
  record within the match radius of it goes. The same rule reaches the deep
  layers, where the ghost carries a different source id at the same place and a
  dedupe by id cannot see it: `build_star_lod.py` drops records within 20
  arcseconds of a filled star — 3 arcsec widened to swallow the tiles'
  octahedral quantisation, which is about 15 — and 330 went.
- **`named.json` has one builder now.** `build_bright.py` used to re-derive the
  named indices against the previous catalogue and refuse to publish if one
  failed to match by position. That guard cannot survive this change: R Doradus
  was a saturated Gaia record at magnitude 1.96 and is now its Hipparcos record
  at 4.66, which is neither the same position nor the same brightness, and
  every Hipparcos record moved by up to two arcminutes when the epochs were
  fixed. What replaces it is stronger and cheaper: `build_names.py` and
  `build_lines.py` each write the **content hash of the catalogue they resolved
  against** into their own manifest, the catalogue's filename carries the same
  hash, and a test refuses a set whose three hashes disagree. A rebuilt
  catalogue beside a stale name list now fails the gate instead of labelling
  the wrong stars plausibly.
- **The hand-written entries needed a HIP number.** The fifty S1 wrote are
  keyed by name where the IAU list also carries the star, which is
  index-independent and safe. The four the IAU list does *not* carry — Alpha
  Centauri, Delta Velorum, R Doradus, Regor — were resolved by a `brightIndex`
  written against the catalogue of the day, so after a rebuild "Regor" would
  name whatever star had slid into slot 32. They carry a `hip` now and resolve
  positionally through HYG, the same way an IAU row does.
- **Two segments stay dropped, both in Canis Major.** They run through HIP
  33165, V 6.65, which is fainter than the catalogue's own magnitude 6.5 limit.
  Raising the limit for one segment would add thousands of records to every
  visitor's first fetch, so the gap is stated instead — in NOTICE, in the
  README and on /about.
- **`build_bright.py` gave up `crossmatch.py` to stay under 400 lines.** Where
  a catalogue row points, how far apart two directions are, and whether some
  indexed star is the star in hand: geometry, which is not merge policy.

### Content addressing

Measured through the edge before the change: `/stars/lod/*` was `immutable` and
served HIT, and *everything else* carried `public, max-age=0` and came back
`cf-cache-status: REVALIDATED` — nine conditional GETs to the origin on every
page load. The band already had a hash in its filename and still got
`max-age=0`. `docs/sky-cdn.md`, written alongside this, measures what that
actually cost: Next answers each with a 304 of a couple of hundred bytes, so
the saving is nine round trips of latency rather than 1.7 MB of egress.

Every fetched asset is now named `<stem>-<sha256[:12]><suffix>`, generated by
the builders: `build_star_lod.py` names the 768 tiles and `g9` from the per-tile
sha256 its manifest already carried (tile id first, so the directory still reads
as a grid), and `tools/starcat/name_assets.py` stamps the catalogue, the names,
the figures, the band's two bakes, the cities, the three earth textures and the
manifest itself, writing the map to `src/features/sky/asset-names.ts`.
`assets.ts` reads that module, Next content-hashes the bundle that imports it,
and `next.config.ts` serves `/stars/*`, `/sky/*`, `/cities/*` and `/textures/*`
`public, max-age=31536000, immutable`. The HTML is the only mutable thing left
in the graph, and no cache ever has to be purged.

A client holding an old bundle asks for a filename this deploy does not have.
That is a 404, and `lod-stream.ts` has always treated a refused tile as a tile
of grain the sky does without; `lod-stream.test.ts` now says so out loud, and
checks the streamer asks for more on the next tick rather than wedging.

Every builder reads through `name_assets.current()` rather than a literal path,
because the file it wrote last time is not where it wrote it any more.

### The detail delta

`sky_star_detail` is 3,087,894 rows and 257 MB compressed. Rebuilding it to
answer for 198 new stars would be a day of Gaia TAP and an hour of loading for
a change of 0.006%, and the rows it would rewrite are byte-identical to the
ones already there. `tools/starcat/build_delta.py` asks `build_detail.py`'s own
routines — HYG, the IAU list, the boundary walk, one 198-id SIMBAD batch — for
exactly the Hipparcos numbers `hip_filled.json` names, and writes
`data/sky_star_detail.delta.csv` (198 rows, 237 KB). `node scripts/load-sky.mjs
--delta` upserts it with `on conflict do update`: under a second, and idempotent.

### The gate, on a server with no GPU

nexus (12 cores, no GPU) runs the suite at 12 workers in 1m52s. Headless
Chromium there has no WebGL2 at all unless it is asked for ANGLE's software
rasteriser, so `playwright.config.ts` gains an opt-in `E2E_SOFTWARE_GL=1` that
passes `--use-gl=angle --use-angle=swiftshader`; without it every sky spec waits
twenty seconds for a pixel that is never lit.

Three specs are then sensitive to how fast that rasteriser is rather than to
what the code does. At 12 workers `sky-lines`, `sky-stars` and the
reduced-motion case fail; at 3 workers the first two pass and at 1 the third
does, which is the contention the Mac's five-worker flake also was. All three
are measuring a *settled* frame — that the figures changed eight thousand
pixels, that Sirius's core reached white, that the sky stopped changing inside
twenty seconds — and a dozen software rasterisers sharing twelve cores do not
settle a frame in the time those assertions allow. (The float target is not the
reason: SwiftShader offers both `EXT_color_buffer_half_float` and
`EXT_color_buffer_float`, so the accumulation buffer is the real one.) A
workstation with a GPU runs the whole suite green; a headless server is where
the worker count has to come down.
