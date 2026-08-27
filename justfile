# rn-site — the commands a developer runs, in the order CI runs them.
# `just` lists them; `just gate` is what `deploy` checks before it ships.

set shell := ["bash", "-euo", "pipefail", "-c"]

bundles := "app-member app-org app-platform starscape"
wasm_crates := "-p rn-api -p rn-ui -p rn-app -p rn-org -p rn-platform -p rn-starscape"

default:
    @just --list --unsorted

# ── gates (mirrors .forgejo/workflows/deploy.yml) ───────────────────────────

# Everything the runner checks before a digest is published.
gate: fmt clippy test check-wasm bench-quick lint-sh size-gate

fmt:
    cargo fmt --check

fix-fmt:
    cargo fmt

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# The four bundle crates under the wasm target, the way trunk compiles them.
clippy-wasm:
    cargo clippy -p rn-ui -p rn-app -p rn-org -p rn-platform --all-targets --target wasm32-unknown-unknown -- -D warnings

test:
    cargo test --workspace --locked

# One crate or one test name: `just t rn-kernel merge`.
t crate *filter:
    cargo test -p {{crate}} -- {{filter}}

check-wasm:
    cargo check --target wasm32-unknown-unknown {{wasm_crates}}

# Kernel budgets with criterion's reduced sampling; the hand-timed lines run in full.
bench-quick:
    cargo bench -p rn-kernel -- --quick --noplot

bench *args:
    cargo bench -p rn-kernel -- {{args}}

lint-sh:
    bash -n deploy/*.sh tools/*.sh tools/perf/*.sh

size-gate:
    tools/size-gate.sh

# ── run ─────────────────────────────────────────────────────────────────────

# The server on :3004, dev mode, bundles and static tree read from disk.
run:
    cargo run -p rn-site

# The same, with the developer sign-in button under the sign-in form. Debug
# builds only — a release binary contains no such route (`auth::dev`).
run-dev:
    RN_SITE__DEV=1 cargo run -p rn-site

# Register the first operator through the real form, then grant it (dev only).
seed:
    tools/seed.sh

# This worktree's own instance: its own ports, state and keys, so two
# checkouts can be up at once (tools/ephemeral.sh).
up:
    tools/ephemeral.sh up

down:
    tools/ephemeral.sh stop

# `just eph status`, `just eph reset`, `just eph ports`.
eph cmd="status":
    tools/ephemeral.sh {{cmd}}

# The rn-ui fixture API on :3199, for `trunk serve` against a bundle.
fixture:
    cargo run -p rn-ui --example fixture

# Live bundle work: `just serve app-member` then open :8080/app.
serve bundle:
    cd crates/{{bundle}} && trunk serve --public-url / --serve-base /

# ── bundles ─────────────────────────────────────────────────────────────────

# Release build of every bundle (rebuild the server afterwards: release embeds dist/, dev reads it from disk).
bundles:
    for b in {{bundles}}; do trunk build --release --config crates/$b/Trunk.toml; done
    @just bundle-sizes

bundle bundle:
    trunk build --release --config crates/{{bundle}}/Trunk.toml

bundle-sizes:
    @for b in {{bundles}}; do ls -l crates/$b/dist/*.wasm 2>/dev/null | awk -v b=$b '{printf "%-13s %8.0f KB  %s\n", b, $5/1024, $9}'; done

# Release server binary with the bundles baked in.
release: bundles
    cargo build --release -p rn-site

# ── e2e ─────────────────────────────────────────────────────────────────────

# Playwright, serially, chromium; needs `just run` on :3004 and `just seed` done once.
e2e *args:
    cd end2end && npx playwright test --workers=1 --project=chromium {{args}}

e2e-install:
    cd end2end && npm ci && npx playwright install chromium

e2e-report:
    cd end2end && npx playwright show-report

# ── cluster, perf, resilience ───────────────────────────────────────────────

# Three real voters on this host (release build), ports 3161–3163.
cluster cmd="start" *args:
    tools/cluster.sh {{cmd}} {{args}}

# oha + samply over the seeded cluster into docs/perf/<today>/.
perf *args:
    tools/perf/run.sh {{args}}

# Kill a voter under load, then a second, then restore.
drill *args:
    tools/perf/drill.sh {{args}}

# ── delivery ────────────────────────────────────────────────────────────────

# The container image, as the runner builds it.
image tag="rn-site:local":
    podman build -t {{tag}} -f Containerfile .

# What deploy checks on each node after a rollout (sha optional).
verify-cluster *sha:
    deploy/verify-cluster.sh {{sha}}

# ── hygiene ─────────────────────────────────────────────────────────────────

# Nothing left running, nothing left over.
residue:
    @pgrep -fl rn-site || echo "no rn-site processes"
    @git status --short || true

# Duplicate crates in the graph.
deps:
    cargo tree -d --workspace

clean:
    cargo clean
    rm -rf crates/*/dist target/cluster

# Serve the docs tree locally (review, perf, plan) — never a Claude artifact.
docs port="8090":
    python3 -m http.server {{port}} -d docs
