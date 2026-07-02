# stage_1

Base template project — fork this to start a new app.

It's an [Axum](https://github.com/tokio-rs/axum) server rendering
[Askama](https://github.com/askama-rs/askama) HTML, with a typed TypeScript +
[Solid](https://www.solidjs.com/) frontend (built by [Vite](https://vite.dev/))
and a [sqlx](https://github.com/launchbadge/sqlx)/sqlite persistence layer
already wired up — extend these pieces rather than replacing them. Agents:
read [AGENTS.md](AGENTS.md) first.

## Layout

    src/
      main.rs         Entry point — loads .env, initializes telemetry, starts the server.
      app.rs          Router assembly, layer stack, server bootstrap.
      config.rs        Environment-sourced runtime configuration.
      state.rs         Shared application state (AppState — store handle, uptime).
      error.rs         Crate-wide error type and its HTTP/JSON representation.
      telemetry.rs     Tracing / logging + per-request span setup.
      openapi.rs        OpenAPI document root.
      view.rs          Template-rendering helper.
      store/           Persistence boundary — one file per table, query macros only.
        mod.rs         Pool setup, migrations.
        guestbook.rs   Demo table's types + queries.
      handlers/        HTTP handlers, one module per feature area.
        home.rs, about.rs, errors.rs, health.rs, guestbook.rs, client_errors.rs
    templates/         Askama HTML templates (`_layout.html` is the base).
    static/
      css/             Stylesheets, split by concern.
      dist/            Vite build output (gitignored).
    migrations/        sqlx migrations, one file per table.
    ts/                TypeScript frontend (bun + vite + Solid).
      src/entries/     One vite entry per bundle (site-wide, per-island).
      src/islands/     Solid components hydrated client-side.
      src/lib/         nav/theme/beacon/api helpers.
      src/generated/   ts-rs bindings for Rust API types (committed).

Add a new handler module per feature area and a matching template/TS entry
rather than growing any single file — see AGENTS.md for the full checklist.

## Running

A fresh clone works with zero setup:

    cargo run

This creates and migrates `data/app.db` automatically. Binds to
`127.0.0.1:3000` by default; override with `BIND_ADDR`:

    BIND_ADDR=0.0.0.0:8080 cargo run

Pages render without it, but islands (e.g. `/guestbook`) need the frontend
built once:

    cd ts && bun install && bun run build

For active development, run both watchers side by side (see AGENTS.md):

    cargo watch -w src -w templates -w migrations -x run
    cd ts && bun run watch

## Tech stack

- Axum / Tokio / Askama / tower-http / tracing
- sqlx + sqlite (persistence), utoipa (OpenAPI docs)
- Vite + TypeScript + SolidJS (frontend), zod (API validation), ts-rs (Rust → TS types)
- bun (frontend package manager/runner)
