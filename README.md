# ronitnath.com

Ronit Nath's site and the small application behind it: a public landing page,
member accounts, events with guest pages, organizations and sharing, and an
operator surface over the whole model. Self-hosted on the internal cluster.

`docs/plan.md` is the binding contract — the stack, the model, the four tiers
and the rung ladder. `docs/design.md` is the design brief; the OKLCH tokens it
names live in `src/app/globals.css`.

## Stack

Next.js 15 (App Router, React 19, TypeScript strict) · Tailwind 4 over CSS
variables · PostgreSQL 17 through `pg` + Drizzle ORM · pnpm · Node 24 in a
`node:24-alpine` standalone image · vitest and Playwright.

Authorisation is four server helpers (`src/lib/tiers.ts`): every page and
action names the tier it needs, so there is no per-route guard to forget.
Internal integer ids never leave the server — `src/lib/ids.ts` turns one into a
type-prefixed, AES-128-encrypted public id.

## The sky

The landing page is the real sky, from a real place, at a real instant, and
every part of that is checkable. `src/features/sky/` owns it.

- **Catalog.** `public/stars/bright.bin` is 12,191 stars — J2000 unit vector,
  magnitude and colour, brightest first — with `named.json` naming 50 of them
  from the IAU and SIMBAD. `public/sky/milkyway.webp` is Gaia star counts on an
  equatorial grid, `public/cities/cities.bin` the filtered GeoNames
  `cities15000`, and `public/textures/earth/` NASA's Blue Marble. All of them
  are carried unchanged from the pre-rebuild site: designed assets, not
  regenerated ones. `public/stars/NOTICE` and `public/textures/earth/NOTICE`
  carry their attribution and must stay with them.
- **Clock.** `clock.ts` runs the sky at 60× wall time from a fixed epoch, with
  the accumulated lead taken modulo one sidereal day so the simulated _date_
  never runs away and the sky either side of that seam is identical. The server
  stamps the instant into the page, so every browser draws the same sky however
  wrong its own clock is.
- **Observer.** One point, shared by everybody: `track.ts` flies a great circle
  inclined 63° through San Francisco, one lap per half sidereal day, in
  Earth-fixed coordinates (`sidereal.ts` composes Earth's rotation exactly once,
  in the view). Dragging or clicking the globe overrides it tab-locally
  (`observer.ts`), travelling 800 ms along the great circle; "Resume orbit"
  travels back. There is no geolocation prompt and no per-visitor sky.
- **Drawing.** Stars and the Milky Way are a 2D canvas (`star-field.ts`) at
  ≤30 fps, painting the magnitude response the old WebGL starscape shipped
  (`tuning.ts`). The mini-globe is the one thing that uses WebGL2
  (`globe-gl.ts`): a textured sphere lit from the simulated instant's subsolar
  point, so its terminator is the real one, falling back to a flat day-texture
  disc where WebGL2 is unavailable.
- **Annotations.** `annotate.ts` projects the named stars through the same view
  and keeps at most three that clear a keep-out band around the hero; on a
  phone, where nothing clears it, the best above-horizon star is forced to the
  edge and says so. `label.ts` writes the grounding caption — position first,
  then the nearest city, "over" it within 50 km.
- **Reduced motion** draws one frame and stops repainting; "Pause sky" freezes
  the clock. Nothing is fetched, decoded or painted until the main thread is
  idle, and every asset is validated before it is drawn: Lighthouse on the
  production build scores 100 (desktop) and 99 (mobile).

No environment variable configures any of this.

## Dev loop

```sh
cp .env.example .env          # set ID_KEY: openssl rand -hex 16
pnpm install
pnpm db:up                    # postgres:17 on 127.0.0.1:5433
pnpm db:migrate               # drizzle-kit; migrations never run at boot
pnpm dev                      # http://localhost:3000
```

`DEV_DB_PORT=5443 pnpm db:up` moves the database when something else on the
machine already holds 5433.

With no `SMTP_URL`, verification and reset mail is logged to stdout and written
to `.mail/<timestamp>.eml` (gitignored); `MAIL_DIR` moves that directory, which
the Playwright run uses because Next's standalone server changes directory.

`pnpm seed:operator --email ronit@isoastra.com` creates the platform operator
with no factors, ready for ZITADEL to attach itself on first sign-in. Adding
`--password <secret>` seeds a _local_ operator instead — a confirmed address, a
password and the `operator` relation — which is how the Playwright run reaches
`/platform` without an identity provider. The allowlisted address never takes
that path.

`RN_IMPERSONATION=off` removes SignInAs entirely: the control is not drawn and
the command declines. The operator commands that destroy or impersonate also
ask for a password (or a fresh ZITADEL round trip) inside the last ten minutes.

`MAIL_FAIL` names a recipient the transport must refuse — `1` for every
address, otherwise a substring of one. The Playwright run sets it so a send
can be watched to fail for one visitor and land for the rest; nothing outside
a test should set it.

- `pnpm gate` — typecheck, lint, unit tests, build. Green before any report.
- `pnpm e2e` — Playwright against the standalone server the image ships.
- `pnpm db:generate` — a new SQL migration after a `src/db/schema.ts` change.
  A change that both drops and adds a column on one table asks, on a terminal,
  whether the new column is a rename; answer it before committing the file.

A `justfile` mirrors these for `just gate`, `just db-up`, `just image`.

## Deploy

Manual `docker compose` on the target host, host network, port 3140.
`deploy/README.md` has the procedure, the config file and the rollback.
