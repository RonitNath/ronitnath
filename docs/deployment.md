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

Exposes port `8080`. Bind-mount or edit `config.toml` to change defaults.

## Configuration

`config.toml` at the project root sets defaults:

```toml
host = "0.0.0.0"
port = 8080
domain = "ronitnath.com"
```

Environment variables override, in this order of precedence:

- `HOST` — bind address
- `PORT` — bind port (must parse as `u16`)
- `DOMAIN` — canonical hostname, surfaced to logs

All three are optional. If `config.toml` is missing, `Config::default()`
values apply.

## Health check

`GET /healthz` returns `200 ok` with no dependencies. Suitable for
container-orchestrator liveness probes.

## Operational notes

- **No persistent state.** No database, no filesystem writes, no secrets.
  Safe to run multiple replicas and redeploy freely.
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
