# Multi-stage build: Vite frontend, Rust release, then a slim runtime with both bins.
FROM oven/bun:1 AS frontend
WORKDIR /app/ts
COPY ts/package.json ts/bun.lock ./
RUN bun install --frozen-lockfile
COPY ts/ ./
RUN mkdir -p /app/static && bun run build

FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY .sqlx .sqlx
COPY src src
COPY templates templates
COPY migrations migrations
COPY .git .git
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin site --bin admin

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /app/target/release/site /app/target/release/admin /usr/local/bin/
COPY static static
COPY --from=frontend /app/static/dist static/dist
ENV BIND_ADDR=0.0.0.0:3130 \
    ADMIN_BIND_ADDR=0.0.0.0:3131 \
    DATABASE_URL=sqlite:data/app.db
CMD ["site"]
