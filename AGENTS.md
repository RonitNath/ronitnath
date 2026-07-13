# ronitnath.com — agent instructions

ronitnath.com is a product fork of stage_2 and retains its full auth model
(identity/account/membership/factor — see README.md). Extend it — never
replace a piece wholesale. One module per feature area (see organization.md
in your cross-machine context, if you have one).

## Dev loop

Find two open ports, then run both Rust bins and the frontend watcher:

    BIND_ADDR=127.0.0.1:<site-port> cargo watch -w src -w templates -w migrations -x 'run --bin site'
    ADMIN_BIND_ADDR=127.0.0.1:<admin-port> cargo watch -w src -w templates -w migrations -x 'run --bin admin'
    cd ts && ./build.sh --watch

Never use a frontend dev server or proxy. Always hit the real axum server
above — standalone `esbuild --watch` just keeps `static/dist` up to date. Hand the
server URL to the operator for verification and leave it running. There's
no seeded account — sign up at `/signup` first.

## Adding a feature (checklist)

1. `sqlx migrate add -rs <name>` — one file per table; `NOT NULL` on the
   sqlite PK (it's nullable by default) or it deserializes as `Option<_>`.
   If the query has a `RETURNING id`, cast it `as "id!: i64"` (the `!`, not
   just `: i64`) — sqlx can't reliably infer non-null on a `RETURNING`
   column against a table with foreign keys and will otherwise ask for an
   `Into<i64>` impl on `Option<i64>` that doesn't exist.
2. **New domain tables get an `account_id` column and every query takes
   one** — see `src/store/guestbook.rs` for the exemplar. This is the one
   rule in this fork with real teeth: a query that forgets to filter by
   account is the canonical multi-tenant data leak.
3. Add query fns to `src/store/<table>.rs` — `query_as!`/`query!` macros only.
4. Add a handler in `src/handlers/`. Any route touching account-owned data
   takes an `AccountScope` extractor (`crate::auth::AccountScope`) instead
   of a raw id — see `handlers::guestbook` for the pattern (page + JSON
   list + JSON create, all scoped). It rejects with a redirect-to-login
   (HTML) or 401 JSON (`/api/*`) when there's no valid session/bearer
   token. Gate admin-only actions with `scope.require(Role::Admin)?` (see
   `handlers::account`). Register the route in `src/app.rs`; JSON API
   handlers get `#[utoipa::path(...)]` and go through `.routes(routes!(...))`
   so they show up in `/api/openapi.json`; page handlers use the plain
   `.route()` pass-through. Mutating routes need a CSRF check: form
   handlers call `csrf::verify(&scope, &form.csrf_token)` against a hidden
   `csrf_token` field; JSON handlers check an `X-CSRF-Token` header the
   same way (see `handlers::guestbook::api_create`) — CSRF is checked
   per-handler, not via router middleware, since a mutating handler almost
   always consumes the body anyway and middleware can't peek at a form
   field without buffering/replaying it. `AccountScope.csrf_token` is
   `None` for bearer-token auth (CSRF-immune by construction), so
   `csrf::verify` always passes there. Routes with no account concept
   (unauthenticated writes like `/api/client-errors`) still go through
   `rate_limit::enforce` the stage_1 way — merge them as their own
   `OpenApiRouter` fragment with a `.route_layer(...)`.
5. Add a template extending `_layout.html`; every template struct needs a
   `current_user: Option<crate::auth::extract::NavUser>` field (pull it via
   the `NavContext` extractor, or from `scope.display_name`/`scope.csrf_token`
   if the route already required `AccountScope`).
6. Frontend: add the new entry to `ts/build.sh` + one `<script type="module">`
   tag on the page template. Mutating
   fetches need the CSRF header — see `csrfToken()` in `ts/src/lib/api.ts`.
7. `cargo test` (regenerates `ts/src/generated/*.ts` from `#[ts(export)]` types)
   — add a router test alongside it (see "Testing & verification" below).
8. `cargo sqlx prepare -- --tests` if you touched a query — the `--tests`
   matters here (see "Database" below), unlike plain stage_1.
9. Screenshot the result (see verification.md if you have it).

## Database

`cargo run` auto-creates `data/app.db` and runs migrations — nothing to set
up on a fresh fork. Query macros compile from the committed `.sqlx/` offline
cache when no `.env` exists.

For schema/query work: `cp .env.example .env`, install sqlx-cli once
(`cargo install sqlx-cli --no-default-features --features sqlite`), then
`sqlx database reset` / `migrate run` as needed — reset freely, there's no
seed data to lose (signup recreates it). Hand-edited migration files mean a
full reset, not `migrate run`. **Always regenerate the offline cache after
changing a query, with `--tests`**:

    cargo sqlx prepare -- --tests

Plain `cargo sqlx prepare` (no `--tests`) only checks non-test code and
silently skips queries that only exist behind `#[cfg(test)]` (e.g.
`Store::create_membership`, `Store::count_audit_events`) — those then fail
to compile for anyone without `DATABASE_URL` set, which is everyone by
default. Commit the updated `.sqlx/`.

Gotcha: if `.env` exists but `data/app.db` doesn't, compiling fails with an
opaque "unable to open database file" — run `cargo run` once to recreate it,
or delete `.env` to fall back to the offline cache.

## Frontend

All client code is TypeScript under `ts/src`, bundled by the standalone
`esbuild` binary via `ts/build.sh` into `static/dist` (gitignored — a fresh
fork needs one `cd ts && ./build.sh` before islands render; pages still work
server-side without it). The frontend build has no JavaScript package manager,
`package.json`, lockfile, or runtime dependencies. Rust API types derive `TS` +
`ToSchema`; their bindings in `ts/src/generated/` are committed and regenerate
via `cargo test`. Fetch helpers use those generated types plus focused runtime
shape assertions for fields the UI reads. Stateful islands use plain DOM APIs;
static pages stay plain HTML/CSS — that includes the auth pages
(login/signup/settings/account), which are plain `<form method="post">`
submissions, not fetch-based. The FOUC-prevention block in `_theme.html` is
deliberately duplicated with `base.css` — keep both in sync.

## Stubs

Anything not built yet gets a loud `warn!("STUB: ...")` (server) or
`console.warn("STUB: ...")` (client) so it's never silently forgotten.

## Observability

Every request gets an `x-request-id` (echoed on error pages as "ref:" and in
log spans). `/healthz` reports version/git hash/uptime. Client JS errors post
to `/api/client-errors` and land in the same log — grep for "client error".
`/api/openapi.json` documents every JSON endpoint. Mutations additionally
write an `audit_log` row (`Store::audit`) — read them at `/account/audit`
(admin+).

## Hardening (inherited from stage_1)

`build_router` (`src/app.rs`) layers in, outermost first: request-id
assignment/propagation, security response headers (`src/security_headers.rs`
— CSP included, hashed against the *actual* inline `<script>`/`<style>` in
`_theme.html` so it can't drift), request tracing, session resolution
(`auth::middleware::attach_session` — see "Auth" below), the error-page
middleware, a per-request timeout (`REQUEST_TIMEOUT_SECS`, default 30s,
bare 408 on expiry), and a request body size cap (`MAX_BODY_BYTES`, default
1 MiB, 413 over). Unauthenticated write routes (just `/api/client-errors`
now — guestbook moved to account-scoped auth) get per-client rate limiting
(`src/rate_limit.rs`, `RATE_LIMIT_PER_MINUTE`, default 10/min; behind
ingress that sets `X-Forwarded-For`, set `TRUSTED_PROXY=true`). The server
drains in-flight requests on SIGTERM/Ctrl+C rather than dropping them.

**Router::layer ordering gotcha**: `build_router` layers are chained
directly on the router (not bundled into one `tower::ServiceBuilder`) —
see the comment above that block in `app.rs` for why, and note that
`Router::layer`'s *last* call becomes the *outermost* wrapper, the
opposite of `ServiceBuilder`. Read that block bottom-to-top to get
request-flow order. `attach_session` must sit outside (added after)
`render_error_pages`, since the error page's nav needs the session context
`attach_session` inserts.

## Auth

The model (identity/account/membership/factor/session) is in README.md and
`docs/plans/2026-07-stage2-hardened-fork-template.md`. Mechanically:

- **`auth::middleware::attach_session`** resolves the session cookie once
  per request (one DB join: session → identity → account → membership) and
  stashes `Option<SessionContext>` in request extensions. Everything else
  — the nav, `AccountScope`, error pages — reads from there instead of
  re-querying, and because it's resolved fresh every request, a revoked
  session or membership takes effect on the very next request, not at next
  login.
- **`auth::AccountScope`** (`src/auth/extract.rs`) is the extractor
  handlers use: identity + active account + role. Accepts either the
  session cookie or an `Authorization: Bearer <token>` header (api_token
  factor) — bearer auth has no session, so `scope.session_id`/
  `scope.csrf_token` are `None` in that case.
- **Sessions** (`src/store/sessions.rs`): token stored only as a sha256
  hash (`auth::session::hash_token`), sliding expiry, `csrf_token` column
  (a second random value, independent of the session token). Cookie name
  is `__Host-session` when `COOKIE_SECURE=true` (i.e. behind TLS), else a
  plain `session` cookie for local HTTP dev.
- **CSRF**: no router middleware (see checklist item 4 above for why) —
  `csrf::verify(&scope, submitted)` compares against `scope.csrf_token`
  with a constant-time comparison.
- **Password** (`src/auth/password.rs`): argon2id via the `argon2` crate.
  Failed logins verify against a fixed dummy hash when the email doesn't
  exist, so "unknown email" and "wrong password" take the same time —
  don't skip that call even though its result is discarded.
- **api_token** (`src/auth/api_token.rs`): not a session — a bearer header
  checked directly against `factors.secret_hash` (sha256 of the raw
  token). Minted from Settings, shown once.
- Building a new factor kind (OIDC, magic link, QR — all phase 2/3 per the
  plan doc): the schema (`pending_auth`, `factors.external_id`) already
  fits them; nothing today generalizes over `password` via a trait since
  there's only one synchronous mechanism to generalize from — introduce
  the `FactorKind` trait (start/finish two-phase protocol, see the plan
  doc) when the second, asynchronous kind actually lands.
- **Phase 1 has no invite flow** — every identity gets exactly one
  membership (its auto-created personal account, role `owner`). Tests that
  need a second identity with a non-owner role on someone else's account
  seed it directly via `Store::create_membership` (`#[cfg(test)]` — no
  production caller yet) plus `test_util::seed_session`.

## Testing & verification

`cargo test` must stay fast (aim for well under a second; argon2 hashing in
the auth tests makes this slower than plain stage_1, currently ~0.9s) and
parallel-safe — use `Store::connect_in_memory()`, never a shared file db.
Collapse small tests into larger flow tests rather than duplicating setup.

**Router tests**: add an HTTP-level test alongside any new route, not just
a store-level one. `src/test_util.rs` builds a full router over an
in-memory db via `test_app()` (returns `(Router, Store)` — the `Store` is
there for seeding data HTTP can't reach yet, like a role-gating test) and
drives it with `tower::ServiceExt::oneshot`. `test_util::signup(...)` runs
the real signup form and returns a ready-to-use `Authed { cookie,
csrf_token }`. See the exemplars in `#[cfg(test)] mod tests` at the bottom
of `src/app.rs` — one per stage_2 auth property (signup→session roundtrip,
wrong-password + audit row, CSRF required on mutation, cross-account
isolation, role gating, session revocation, bearer-token auth + its own
revocation, factor add/remove + last-factor guard) plus the carried-over
stage_1 hardening exemplars. Copy the pattern rather than inventing a new one.

No playwright yet; add it when the app grows a real multi-step user flow —
login → mutate → logout is stage_1's stated "real multi-step user flow"
threshold and stage_2 now has it. Until then, verify visually
(agent-browser or equivalent): check the relevant breakpoints and both
themes for anything layout- or theme-related.

## Deployment

Production packaging and service lifecycle live in `flake.nix` and `deploy/`;
follow `docs/deploy.md` for build, cutover, rollback, data paths, and bind
boundaries. The Nix/systemd path supersedes Docker Compose after cutover.

## Merge discipline (fork hygiene)

`git remote -v` should show `upstream` pointing at stage_2 — `git fetch
upstream && git merge upstream/main` periodically. Keep product features in
new feature-area modules. The intentional fork seams are `src/bin/{site,admin}.rs`,
router composition in `src/app.rs`, the two bind addresses in `src/config.rs`,
and deployment/identity files. Preserve the public/admin trust-boundary split
when resolving upstream changes: site has no session middleware or auth routes;
admin retains the full stage_2 auth surface.
