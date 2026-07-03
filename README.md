# stage_2

The hardened fork of [stage_1](https://github.com/RonitNath/stage_1) — fork
this instead of stage_1 when the app is multi-user or internet-facing.
Everything stage_1 gives you (Axum + Askama, typed TypeScript + Solid
frontend, sqlx/sqlite, hardening middleware) plus a full identity/account/
factor auth model on top. Agents: read [AGENTS.md](AGENTS.md) first.

stage_2 tracks stage_1 upstream via `git merge` (see AGENTS.md's "Merge
discipline") — it's a fork, not a copy.

## The auth model

Four concepts, kept deliberately separate:

- **identity** — the acting entity (human, agent, or service).
- **account** — the unit of legal ownership. All domain data (e.g.
  `guestbook_entries`) FKs to an account, never to an identity directly.
- **membership** — the edge `(identity_id, account_id, role)`. Role only
  ever means something per-account.
- **factor** — a login mechanism attached to an identity (`password`,
  `api_token` today; `oidc`/`magic_link`/`qr_device` are pluggable additions
  for later phases).

Full design rationale in `docs/plans/2026-07-stage2-hardened-fork-template.md`.

## Layout

    src/
      main.rs         Entry point — loads .env, initializes telemetry, starts the server.
      app.rs          Router assembly, layer stack, server bootstrap.
      config.rs       Environment-sourced runtime configuration.
      state.rs        Shared application state (AppState — store handle, uptime, AuthConfig).
      error.rs        Crate-wide error type and its HTTP/JSON representation.
      security_headers.rs  CSP + baseline security response headers.
      rate_limit.rs   Per-client rate limiting for unauthenticated writes.
      telemetry.rs    Tracing / logging + per-request span setup.
      openapi.rs      OpenAPI document root.
      view.rs         Template-rendering helper.
      test_util.rs    Router-test harness (in-memory app + oneshot + auth helpers).
      auth/           Auth business logic — no SQL here, only store/handler orchestration.
        password.rs, session.rs, csrf.rs, api_token.rs, login.rs, middleware.rs, extract.rs
      store/          Persistence boundary — one file per table, query macros only.
        mod.rs        Pool setup, migrations, the signup transaction.
        identities.rs, accounts.rs, memberships.rs, factors.rs, sessions.rs, audit.rs
        guestbook.rs  Demo table's types + queries — the account-scoping exemplar.
      handlers/       HTTP handlers, one module per feature area.
        home.rs, about.rs, errors.rs, health.rs, guestbook.rs, client_errors.rs
        auth.rs       signup/login/logout pages + forms.
        settings.rs   Factors, api tokens, active sessions.
        account.rs    Account rename + audit log — the role-gating exemplar.
    templates/        Askama HTML templates (`_layout.html` is the base).
      auth/           login.html, signup.html.
    static/
      css/            Stylesheets, split by concern.
      dist/           Vite build output (gitignored).
    migrations/       sqlx migrations, one file per table.
    ts/               TypeScript frontend (bun + vite + Solid).
      src/entries/    One vite entry per bundle (site-wide, per-island).
      src/islands/    Solid components hydrated client-side.
      src/lib/        nav/theme/beacon/api helpers (api.ts attaches the CSRF header).
      src/generated/  ts-rs bindings for Rust API types (committed).

Add a new handler module per feature area and a matching template/TS entry
rather than growing any single file — see AGENTS.md for the full checklist.

## Running

A fresh clone works with zero setup:

    cargo run

This creates and migrates `data/app.db` automatically. Binds to
`127.0.0.1:3000` by default; override with `BIND_ADDR`:

    BIND_ADDR=0.0.0.0:8080 cargo run

Sign up at `/signup` — there's no seeded demo account (a hardened template
shouldn't ship baked-in credentials). Pages render without the frontend
built, but islands (e.g. `/guestbook`) need it once:

    cd ts && bun install && bun run build

For active development, run both watchers side by side (see AGENTS.md):

    cargo watch -w src -w templates -w migrations -x run
    cd ts && bun run watch

## Hardening

Everything stage_1 has (security headers + hashed CSP, per-client rate
limiting on unauthenticated writes, request timeout, body-size cap,
graceful shutdown) plus, on top: sessions, CSRF, password + api-token auth,
account-scoped data access enforced through an extractor rather than trust,
and an audit log. See AGENTS.md's "Auth" section for the full rundown and
env vars.

## Tech stack

- Axum / Tokio / Askama / tower-http / tracing
- sqlx + sqlite (persistence), utoipa (OpenAPI docs)
- argon2 (password hashing), axum-extra (cookies)
- Vite + TypeScript + SolidJS (frontend), zod (API validation), ts-rs (Rust → TS types)
- bun (frontend package manager/runner)
