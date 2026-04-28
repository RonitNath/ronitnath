# syntax=docker/dockerfile:1.7

# ---------- UI build ----------
FROM oven/bun:1 AS ui-builder
WORKDIR /app/ui
COPY ui/package.json ui/bun.lock* ./
RUN --mount=type=cache,target=/root/.bun/install/cache,sharing=locked \
    bun install --frozen-lockfile || bun install
COPY ui/ ./
RUN bun run check && bun run build

# ---------- Rust build (cargo-chef + cache mounts) ----------
FROM rust:1.94-slim-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS rust-builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked --bin ronitnath

# ---------- Runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN mkdir -p /app/data && chown 1000:1000 /app/data
COPY --from=rust-builder /app/target/release/ronitnath /usr/local/bin/ronitnath
COPY --chmod=0644 templates ./templates
COPY --chmod=0644 static ./static
COPY --chmod=0644 config.toml ./config.toml
COPY --from=ui-builder --chmod=0644 /app/ui/dist ./ui/dist
RUN chmod -R a+rX /app/templates /app/static /app/ui
ENV HOST=0.0.0.0 PORT=8080
EXPOSE 8080
USER 1000:1000
CMD ["ronitnath"]
