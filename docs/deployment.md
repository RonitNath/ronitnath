# Deployment

## Container

`Dockerfile` is multi-stage:

1. **`oven/bun:1`** — installs UI deps, runs `bun run check && bun run build`.
2. **`rust:1.94-slim-bookworm`** — compiles the release binary.
3. **`debian:bookworm-slim`** — runtime. Copies the binary, templates,
   static assets (including `fonts/agency-bold.ttf` and `favicon.png`),
   `config.toml`, and `ui/dist/`. Runs as `UID 1000`.

Build:

```sh
docker build -t ronitnath:latest .
```

Run:

```sh
docker run --rm -p 8080:8080 ronitnath:latest
```

Exposes port `8080`. Bind-mount `/app/data` if event state should survive
container replacement.

## Configuration

`config.toml` at the project root sets defaults:

```toml
host = "0.0.0.0"
port = 8080
domain = "ronitnath.com"
database_url = "sqlite://./data/ronitnath.db"
public_base_url = "https://ronitnath.com"
```

Environment variables override, in this order of precedence:

- `HOST` — bind address
- `PORT` — bind port (must parse as `u16`)
- `DOMAIN` — canonical hostname, surfaced to logs
- `DATABASE_URL` — SQLite URL; live compose uses
  `sqlite:///app/data/ronitnath.db`
- `PUBLIC_BASE_URL` — canonical URL used when generating RSVP/signup links
- `EVENT_TOKEN_SECRET` — HMAC/session secret
- `ISOASTRA_*`, `SESSION_*`, `ADMIN_*` — auth and admin allowlist settings

All are optional. If `config.toml` is missing, `Config::default()` values
apply.

## Health check

`GET /healthz` returns `200 ok` with no dependencies. Suitable for
container-orchestrator liveness probes.

## Operational notes

- **Persistent state.** Events and sessions are SQLite-backed. The live
  compose file mounts `ronitnath-data` at `/app/data`; do not bake local
  `data/` into images or commits.
- **Template & asset baking.** Askama templates compile into the binary at
  build time, so runtime has no template filesystem dependency — but
  `ui/dist/` and `static/` are read from disk to serve hashed assets and
  the custom font. A rebuild requires a restart.
- **Font asset.** `Agency Bold` is served from `/static/fonts/agency-bold.ttf`
  and is preloaded in the base template. Verify the font file is present
  in the image after a build (`docker run --rm --entrypoint ls ronitnath
  /app/static/fonts`).
- **TLS termination.** Expected upstream (Cloudflare, nginx, etc.). The
  server speaks plain HTTP.
- **Logs.** Structured via `tracing` — `tracing_subscriber`'s `EnvFilter`
  reads `RUST_LOG`. Default: `info`.

## Cache layout notes

The Dockerfile uses `cargo-chef` and BuildKit cache mounts to keep rebuilds
small on incremental changes:

| Scenario | What rebuilds | What stays cached |
|----------|---------------|-------------------|
| Rust source edit | `cargo build --release` for the top crate only | cargo-chef `cook` dep layer, ui-builder, apt, registry/git mounts |
| Solid or CSS edit | `bun run build` | `bun install` layer (cache mount warm), entire rust stage |
| `package.json` / `bun.lock` change | `bun install` (warm via `/root/.bun/install/cache` mount) + `bun run build` | entire rust stage |
| `Cargo.lock` change | cargo-chef `cook` + final `cargo build` (registry mount warm) | ui-builder, apt |

## sqlx offline mode (for when the DB comes back)

If ronitnath grows a database layer with sqlx queries, build-time query
verification must not require a live DB. The pattern:

1. Run `cargo sqlx prepare --workspace` locally (or in CI with a DB). This
   generates `.sqlx/query-*.json` at the repo root.
2. Commit `.sqlx/` to git.
3. In the `rust-builder` stage, add:
   ```dockerfile
   COPY .sqlx ./.sqlx
   ENV SQLX_OFFLINE=true
   ```
   (Place these before `cargo build`.)
4. Docker builds then compile without any DB connection. If a `.env` file
   with `DATABASE_URL` leaks into the build context, it will take precedence
   over `.sqlx` — `SQLX_OFFLINE=true` is the guard against that.

`SQLX_OFFLINE_DIR` can relocate `.sqlx` when a build system filters it
(e.g., Nix vendoring); not needed for plain Docker.
