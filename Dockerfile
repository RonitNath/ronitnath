# syntax=docker/dockerfile:1.7

# ---------- UI build ----------
FROM oven/bun:1 AS ui-builder
WORKDIR /app/ui
COPY ui/package.json ui/bun.lock* ./
RUN bun install --frozen-lockfile || bun install
COPY ui/ ./
RUN bun run check && bun run build

# ---------- Rust build ----------
FROM rust:1.94-slim-bookworm AS rust-builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY src ./src
COPY templates ./templates
RUN cargo build --release --locked

# ---------- Runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /app/target/release/ronitnath /usr/local/bin/ronitnath
COPY templates ./templates
COPY static ./static
COPY config.toml ./config.toml
COPY --from=ui-builder /app/ui/dist ./ui/dist
ENV HOST=0.0.0.0 PORT=8080
EXPOSE 8080
USER 1000:1000
CMD ["ronitnath"]
