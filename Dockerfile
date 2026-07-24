# syntax=docker/dockerfile:1.7

# All external build inputs are pinned by manifest-list digest. Keep the Rust
# version identical in the planner, dependency, and application stages: Cargo
# Chef recipes are not portable across Rust versions or working directories.
ARG CARGO_CHEF_IMAGE=docker.io/lukemathwalker/cargo-chef@sha256:1689f62cfaa6603480356923cb5966544b2dd6ea523e30486bee4f149965d5bc
ARG NODE_IMAGE=docker.io/library/node@sha256:6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3
ARG RUNTIME_IMAGE=docker.io/library/debian@sha256:63a496b5d3b99214b39f5ed70eb71a61e590a77979c79cbee4faf991f8c0783e

FROM ${CARGO_CHEF_IMAGE} AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS rust-deps
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=ronitnath-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    cargo chef cook --release --locked --recipe-path recipe.json

FROM ${NODE_IMAGE} AS ui-deps
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN --mount=type=cache,id=ronitnath-pnpm-store,target=/pnpm/store,sharing=locked \
    pnpm config set store-dir /pnpm/store \
    && pnpm fetch --frozen-lockfile

FROM ui-deps AS ui-build
COPY ts ts
COPY vite.config.ts ./
RUN --mount=type=cache,id=ronitnath-pnpm-store,target=/pnpm/store,sharing=locked \
    pnpm config set store-dir /pnpm/store \
    && pnpm install --offline --frozen-lockfile \
    && pnpm check \
    && pnpm build

FROM chef AS rust-build
ENV SQLX_OFFLINE=true
COPY --from=rust-deps /app/target /cargo-chef-target
COPY --from=rust-deps /usr/local/cargo /usr/local/cargo
COPY Cargo.toml Cargo.lock build.rs ./
COPY .sqlx .sqlx
COPY migrations migrations
COPY src src
COPY templates templates
RUN --mount=type=cache,id=ronitnath-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=ronitnath-cargo-target,target=/app/target,sharing=locked \
    if [ ! -e target/.cargo-chef-seeded ]; then \
         cp -a /cargo-chef-target/. target/; \
         touch target/.cargo-chef-seeded; \
       fi \
    && cargo build --release --locked --bin site --bin admin \
    && install -D -m 0755 target/release/site /out/site \
    && install -D -m 0755 target/release/admin /out/admin

FROM ${RUNTIME_IMAGE} AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends busybox ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-build /out/site /app/bin/site
COPY --from=rust-build /out/admin /app/bin/admin
COPY static /app/static
COPY --from=ui-build /app/static/dist /app/static/dist

ARG SOURCE_GIT_HASH
RUN test -n "$SOURCE_GIT_HASH"
LABEL org.opencontainers.image.source="https://git.isoastra.com/ronitnath/ronitnath" \
      org.opencontainers.image.revision="${SOURCE_GIT_HASH}" \
      org.opencontainers.image.title="ronitnath.com"

ENV BIND_ADDR=0.0.0.0:3130 \
    ADMIN_BIND_ADDR=0.0.0.0:3131 \
    DATABASE_URL=sqlite:/state/app.db \
    PHOTO_STORAGE_DIR=/state/photos \
    OIDC_PROVIDERS_PATH=/run/config/oidc_providers.json \
    RELEASE_REVISION=${SOURCE_GIT_HASH}

# Nexus's dedicated ronitnath-app identity is 986:985. Numeric ownership also
# lets Docker validate the intended host state directory before process start.
USER 986:985
EXPOSE 3130 3131
CMD ["/app/bin/site"]
