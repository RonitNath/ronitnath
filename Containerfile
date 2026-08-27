# syntax=docker/dockerfile:1.7
#
# Built by the CD release job on alien's privileged deploy runner and pushed to
# the forge registry as an immutable digest (procedures/cd.md). Both base images
# are pinned by manifest-list digest — a tag is a locator, not an artifact.
#
# Incremental by design, per the 2026-07-14 owner ruling: cache mounts keep the
# cargo registry, the build target dir and the trunk install warm across
# releases. Clean-room release builds are deliberately not the routine path.

ARG RUST_IMAGE=docker.io/library/rust@sha256:3382bd20aa942806c533e9a73cd000474fb3ef173f71e684cc9b942675781769
ARG RUNTIME_IMAGE=docker.io/library/debian@sha256:3a39a0592364683e6bab97937b72cad5a8fa6dcbbee90edb3bb48c7f8e94f258

FROM ${RUST_IMAGE} AS build
WORKDIR /app

# trunk is pinned to the same version the devshell provides, so a local build
# and the image build produce the same bundle layout. wasm-bindgen and wasm-opt
# are fetched by trunk itself and pinned here for the same reason: an unpinned
# tool in the middle of a release build is an unpinned input.
ARG TRUNK_VERSION=0.21.14
ENV TRUNK_TOOLS_WASM_BINDGEN=0.2.127 \
    TRUNK_TOOLS_WASM_OPT=version_123
RUN --mount=type=cache,id=rn-site-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rn-site-trunk,target=/trunk-target,sharing=locked \
    rustup target add wasm32-unknown-unknown \
    && CARGO_TARGET_DIR=/trunk-target \
       cargo install trunk --locked --version "${TRUNK_VERSION}"

COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY templates templates
COPY static static

# Stamped into the binary and reported by /readyz and /version, so "which
# revision is nyc serving" has an answer that does not require ssh.
ARG SOURCE_GIT_HASH=unknown
ENV SOURCE_GIT_HASH=${SOURCE_GIT_HASH}

# Bundles first, binary second: the server embeds crates/<bundle>/dist with
# rust-embed at compile time, so a binary built before the bundles exist would
# ship an empty frontend and still pass its own build.
RUN --mount=type=cache,id=rn-site-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rn-site-cargo-target,target=/app/target,sharing=locked \
    for bundle in app-member app-org app-platform starscape; do \
        trunk build --release "crates/$bundle/index.html" || exit 1; \
    done \
    && cargo build --release --locked --bin rn-site \
    && mkdir -p /out/bin \
    && cp target/release/rn-site /out/bin/rn-site \
    && cp -r static /out/static

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
COPY --from=build /out/static /app/static

# COPY preserves the build context's modes, and a checkout made under a 0077
# umask yields 0600 assets that the non-root runtime user cannot read — the
# site then 404s every star and sky asset, which reads as a routing bug and is
# not one.
RUN chmod -R a+rX /app/static && chmod 0755 /app/bin/rn-site

# Re-declared here on purpose: an ARG is scoped to the stage that declares it,
# so setting it only in the build stage left every running node reporting
# version "dev" — which is exactly the question /readyz exists to answer.
ARG SOURCE_GIT_HASH=unknown
ENV SOURCE_GIT_HASH=${SOURCE_GIT_HASH} \
    RN_SITE__STATIC_DIR=/app/static \
    RN_SITE__ADDR=0.0.0.0:3160 \
    RN_SITE__MODE=prod \
    RUST_LOG=info

USER 9751:9751
EXPOSE 3160
ENTRYPOINT ["/app/bin/rn-site"]
