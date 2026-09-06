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
`--password <secret>` seeds a *local* operator instead — a confirmed address, a
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
