# syntax=docker/dockerfile:1.7
#
# Built by the CD release job on alien's privileged deploy runner and pushed to
# the forge registry as an immutable digest (procedures/cd.md). Both base images
# are pinned by manifest-list digest — a tag is a locator, not an artifact.
#
# Incremental by design, per the 2026-07-14 owner ruling: cache mounts keep the
# cargo registry, the build target dir and the cargo-leptos install warm across
# releases. Clean-room release builds are deliberately not the routine path.

ARG RUST_IMAGE=docker.io/library/rust@sha256:3382bd20aa942806c533e9a73cd000474fb3ef173f71e684cc9b942675781769
ARG RUNTIME_IMAGE=docker.io/library/debian@sha256:3a39a0592364683e6bab97937b72cad5a8fa6dcbbee90edb3bb48c7f8e94f258

FROM ${RUST_IMAGE} AS build
WORKDIR /app

# cargo-leptos is pinned to the same version the devshell provides, so a local
# `nix develop` build and the image build produce the same bundle layout.
ARG CARGO_LEPTOS_VERSION=0.3.7
RUN --mount=type=cache,id=rn-site-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rn-site-cargo-leptos,target=/cargo-leptos-target,sharing=locked \
    rustup target add wasm32-unknown-unknown \
    && CARGO_TARGET_DIR=/cargo-leptos-target \
       cargo install cargo-leptos --locked --version "${CARGO_LEPTOS_VERSION}"

# cargo-leptos fetches these three tools itself and, left alone, resolves each to
# whatever is newest at build time — an unpinned input in the middle of a release
# build. style/tailwind.css is a real `@import "tailwindcss"`, so the CSS this
# site ships would drift with it. Pin them.
ENV LEPTOS_TAILWIND_VERSION=v4.2.1 \
    LEPTOS_WASM_OPT_VERSION=version_123

COPY Cargo.toml Cargo.lock ./
COPY src src
COPY style style
COPY public public

# Stamped into the binary and reported by /readyz and /version, so "which
# revision is nyc serving" has an answer that does not require ssh.
ARG SOURCE_GIT_HASH=unknown
ENV SOURCE_GIT_HASH=${SOURCE_GIT_HASH}

RUN --mount=type=cache,id=rn-site-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rn-site-cargo-target,target=/app/target,sharing=locked \
    cargo leptos build --release \
    && mkdir -p /out/bin \
    && cp target/release/rn-site /out/bin/rn-site \
    && cp -r target/site /out/site

FROM ${RUNTIME_IMAGE} AS runtime

# curl is only here for the Compose healthcheck; the image root filesystem is
# read-only at runtime, so nothing else can be added later.
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# A container uid is only ever numeric, and the same number is a different
# account on each host — which is how another app's identity gets handed to a
# service by accident. 9751 was verified unassigned on nexus, nyc and delenda;
# 9750 is universe's and is deliberately not inherited.
RUN groupadd --system --gid 9751 rn-site \
    && useradd --system --uid 9751 --gid 9751 --home-dir /app --no-create-home rn-site

WORKDIR /app
COPY --from=build /out/bin/rn-site /app/bin/rn-site
COPY --from=build /out/site /app/site

# COPY preserves the build context's modes, and a checkout made under a 0077
# umask yields 0600 assets that the non-root runtime user cannot read — the
# site then 404s every stylesheet and sky asset while /pkg (written by
# cargo-leptos itself) works, which reads as a routing bug and is not one.
RUN chmod -R a+rX /app/site && chmod 0755 /app/bin/rn-site

# Re-declared here on purpose: an ARG is scoped to the stage that declares it,
# so setting it only in the build stage left every running node reporting
# version "dev" — which is exactly the question /readyz exists to answer.
ARG SOURCE_GIT_HASH=unknown
ENV SOURCE_GIT_HASH=${SOURCE_GIT_HASH} \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:3160 \
    LEPTOS_ENV=PROD \
    RUST_LOG=info

USER 9751:9751
EXPOSE 3160
ENTRYPOINT ["/app/bin/rn-site"]
