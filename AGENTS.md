# stage_1 — agent instructions

This is a combined stage-1/stage-2 skeleton: mock UI *and* the full typed
stack it grows into. Extend it — never replace a piece wholesale. One module
per feature area (see organization.md in your cross-machine context, if you
have one).

## Dev loop

Find an open port, then run both watchers and keep them alive:

    BIND_ADDR=127.0.0.1:<port> cargo watch -w src -w templates -w migrations -x run
    cd ts && bun run watch

Never use `vite dev` or a dev-server proxy. Always hit the real axum server
above — `vite build --watch` just keeps `static/dist` up to date. Hand the
server URL to the operator for verification and leave it running.

## Adding a feature (checklist)

1. `sqlx migrate add -rs <name>` — one file per table; `NOT NULL` on the
   sqlite PK (it's nullable by default) or it deserializes as `Option<_>`.
2. Add query fns to `src/store/<table>.rs` — `query_as!`/`query!` macros only.
3. Add a handler in `src/handlers/`, register the route in `src/app.rs`. JSON
   API handlers get `#[utoipa::path(...)]` and go through
   `.routes(routes!(...))` so they show up in `/api/openapi.json`; page
   handlers use the plain `.route()` pass-through. Unauthenticated write
   routes should go through `rate_limit::enforce` (merge them as their own
   `OpenApiRouter` fragment with a `.route_layer(...)`, as the guestbook
   and client-error routes do in `app.rs` — a bare `.routes(...)` call
   isn't rate-limited by default).
4. Add a template extending `_layout.html`.
5. Frontend: new vite entry in `ts/vite.config.ts` (`build.rollupOptions.input`)
   + one `<script type="module">` tag on the page template.
6. `cargo test` (regenerates `ts/src/generated/*.ts` from `#[ts(export)]` types)
   — add a router test alongside it (see "Testing & verification" below).
7. `cargo sqlx prepare` if you touched a query.
8. Screenshot the result (see verification.md if you have it).

## Database

`cargo run` auto-creates `data/app.db` and runs migrations — nothing to set
up on a fresh fork. Query macros compile from the committed `.sqlx/` offline
cache when no `.env` exists.

For schema/query work: `cp .env.example .env`, install sqlx-cli once
(`cargo install sqlx-cli --no-default-features --features sqlite`), then
`sqlx database reset` / `migrate run` as needed — reset freely, the seed
migration restores continuity. Hand-edited migration files mean a full
reset, not `migrate run`. **Always `cargo sqlx prepare` after changing a
query** and commit the updated `.sqlx/`.

Gotcha: if `.env` exists but `data/app.db` doesn't, compiling fails with an
opaque "unable to open database file" — run `cargo run` once to recreate it,
or delete `.env` to fall back to the offline cache.

## Frontend

All client code is TypeScript under `ts/src`, built by vite into
`static/dist` (gitignored — a fresh fork needs one `cd ts && bun install &&
bun run build` before islands render; pages still work server-side without
it). Use `bun`, not `npm`/`node`. Zod-parse every API response
(`ts/src/lib/api.ts`); Rust API types derive `TS` + `ToSchema` and their
bindings in `ts/src/generated/` are committed — regenerate via `cargo test`,
and keep zod schemas written `satisfies z.ZodType<Generated>` so drift is a
TS compile error, not a runtime surprise. Solid islands only for stateful
UI; static pages stay plain HTML/CSS. The FOUC-prevention block in
`_theme.html` is deliberately duplicated with `base.css` — keep both in sync.

## Stubs

Anything not built yet gets a loud `warn!("STUB: ...")` (server) or
`console.warn("STUB: ...")` (client) so it's never silently forgotten.

## Observability

Every request gets an `x-request-id` (echoed on error pages as "ref:" and in
log spans). `/healthz` reports version/git hash/uptime. Client JS errors post
to `/api/client-errors` and land in the same log — grep for "client error".
`/api/openapi.json` documents every JSON endpoint.

## Hardening

`build_router` (`src/app.rs`) layers in, outermost first: request-id
assignment/propagation, security response headers (`src/security_headers.rs`
— CSP included, hashed against the *actual* inline `<script>`/`<style>` in
`_theme.html` so it can't drift), request tracing, the error-page
middleware, a per-request timeout (`REQUEST_TIMEOUT_SECS`, default 30s,
bare 408 on expiry — it fires *before* the error-page middleware, so it
can't render the templated page, just a plain response), and a request
body size cap (`MAX_BODY_BYTES`, default 1 MiB, 413 over). Unauthenticated
write routes additionally get per-client rate limiting
(`src/rate_limit.rs`, `RATE_LIMIT_PER_MINUTE`, default 10/min; behind
ingress that sets `X-Forwarded-For`, set `TRUSTED_PROXY=true` or every
client shares the ingress's own IP). The server drains in-flight requests
on SIGTERM/Ctrl+C rather than dropping them.

**Router::layer ordering gotcha**: `build_router` layers are chained
directly on the router (not bundled into one `tower::ServiceBuilder`) —
see the comment above that block in `app.rs` for why, and note that
`Router::layer`'s *last* call becomes the *outermost* wrapper, the
opposite of `ServiceBuilder`. Read that block bottom-to-top to get
request-flow order.

None of this assumes an authenticated actor — sessions, CSRF, and
audit-log attribution are deliberately out of scope for this base
template; see `docs/plans/` (if still present) for the hardened fork that
adds them.

## Testing & verification

`cargo test` must stay fast (<~1s) and parallel-safe — use
`Store::connect_in_memory()`, never a shared file db. Collapse small tests
into larger flow tests rather than duplicating setup.

**Router tests**: add an HTTP-level test alongside any new route, not just
a store-level one. `src/test_util.rs` (`#[cfg(test)]`) builds a full
router over an in-memory db via `test_app()` and drives it with
`tower::ServiceExt::oneshot` — no listener, no port, still inside the
<~1s budget. See the exemplars in `#[cfg(test)] mod tests` at the bottom
of `src/app.rs` (one per assertion style: page render, templated 404 +
request-id, JSON roundtrip through the store, validation error shape,
body-limit 413, security headers + CSP drift guard, rate-limit 429) and
copy the pattern rather than inventing a new one.

No playwright yet; add it when the app grows a real multi-step user flow.
Until then, verify visually (agent-browser or equivalent): check the
relevant breakpoints and both themes for anything layout- or
theme-related.
