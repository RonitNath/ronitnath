# ronitnath.com Site

Ronit Nath's personal site. Rust + Leptos (islands mode) + Axum, one binary,
served from three replicas behind the public edge.

The landing page is ronitnath.com. Options:
- Authenticate

## `/auth`

`/auth` registers and signs identities in against the replicated Hiqlite store.
The browser receives a high-entropy opaque bearer token; only its SHA-256 digest
is persisted. Session identity, capabilities, expiry, and revocation remain
server-side, so every replica observes the same session and sign-out is
immediate. Auth responses are `no-store`; production cookies are `Secure`,
`HttpOnly`, and `SameSite=Lax`.

Auth model (schema in `migrations/1_auth.sql`, types in `src/auth/`):
- Identifiers
    - `id` INTEGER — server-local join key; never crosses to the client
    - `public_id` UUID v4 — opaque external id; resolved via in-process hashmap
- Auth Factors
    - Referential:
        - Email (`verified_at` column kept; verification gate warns and skips for now)
    - Providence
        - Password (argon2id PHC)
    - External
        - OIDC/Oauth2 (deferred)
- Identity
    - Who literally is this person?
    - What kind of entity are they? (`person` | `service`)
- Accounts
    - `primary` (default ownership boundary; was "personal"), `business`, `shared`, `alternate`, `service`
    - Legal ownership boundary
- Access
    - What capabilities does this person have?
- Session
    - Where is this account authorized to act currently?

Dev-only: `cargo … --features dev-dashboard` enables hiqlite’s query UI. Do not
ship that feature on the public edge.

## Ops surface

| Route | Purpose |
|---|---|
| `/healthz` | Dependency-free liveness. What the edge's active health check polls and what Compose's healthcheck uses. |
| `/readyz` | JSON `{status, node, version}` — answers "which revision is nyc serving" without an ssh. The rollout asserts every replica reports the same `version`. |
| `/version` | The release stamp on its own. |

`version` comes from `SOURCE_GIT_HASH`, stamped into the image at build time;
`node` comes from `RN_SITE_NODE` in the host's `node.env`.

## Development

```sh
nix develop            # pins rust, the wasm target and cargo-leptos
cargo run              # self-contained: create data/ → Hiqlite → migrate → serve
cargo leptos watch     # same app with hydrate rebuilds; http://127.0.0.1:3000
```

`config.toml` defaults to `mode = "dev"` and `db_path = "data/db.sqlite"`.
Override with `RN_SITE__MODE` / `RN_SITE__DB_PATH`, or point at another file with
`RN_SITE_CONFIG`. In dev the process creates the data directory if missing;
Hiqlite owns and creates its nested SQLite state-machine file. Production
requires the data directory to be provisioned and mounted before startup. The
process then applies `migrations/`. Ctrl+C / SIGTERM runs the
axum graceful-shutdown path in `src/operations/shutdown.rs` (hiqlite teardown
lives in the same `select!`).

The gates that CI enforces, runnable locally:

```sh
cargo fmt --check
cargo clippy --no-default-features --features ssr --all-targets -- -D warnings
cargo test --locked --no-default-features --features ssr
```

Package `default = ["ssr"]` so `cargo run` works; `hydrate` is the wasm feature
set and is selected by cargo-leptos (`lib-default-features = false`).

### Starscape telemetry

Open `/?debug=telemetry` to enable bounded, in-memory diagnostics for a
long-running tab. Inspect `window.__rnTelemetry` in the browser console. It
reports star-stream progress/completion, starscape frame gaps and WebGL context
loss, plus globe tick gaps, heap usage (where the browser exposes it), and
renderer geometry/texture/program counts. The telemetry is absent on ordinary
page loads and retains at most 100 lifecycle events.

The home page's named-star callouts use a curated 50-star Hipparcos/SIMBAD
cross-match. Activating a callout or **Load StarScape** dynamically imports the
celestial-atlas module; neither that module nor its Gaia LOD manifest is fetched
before interaction. The atlas progressively adds the checked-in Gaia DR3
`G<=9` and region-addressable `G<=12` assets under `public/stars/lod/`.

Refresh those generated assets explicitly with `python3 tools/build_star_lod.py`.
The generator queries the official Gaia DR3 archive in bounded, resumable sky
tiles with limited parallelism, then writes a versioned binary and manifest. The deployed site never
contacts Gaia, SIMBAD, or VizieR at runtime.

## Deployment

Push to `deploy` runs Forgejo Actions: gates on the zero-secret native runner,
then a privileged release job that builds `Containerfile`, publishes an immutable
digest to the forge registry, and rolls it across **nexus, nyc and delenda** with
the fleet `rn-site-deploy` playbook. `deploy/provision.sh` prepares the hosts and
is deliberately *not* part of CD — an operator provisions, the pipeline only ever
rewrites a digest.

The production service is one three-voter Hiqlite cluster, not three independent
SQLite files. `deploy/provision.sh` creates each node's persistent `state/`
directory, installs the shared Raft/API secrets, and writes stable node IDs plus
the full mesh peer map. Run it before the first stateful release or after a
topology/Compose change. Routine CD requires all three voters ready, rolls one
node at a time through the fleet playbook, then proves every node reports the
released revision. The first bootstrap is an operator action: provision all
hosts, then start nodes 1, 2, and 3 in that order before enabling traffic.

Raft/API ports 8100/8200 are bound only to each host's mesh IP and must be
restricted by host/mesh ACLs to the rn-site peers. The complete `state/`
directory—not merely the nested SQLite file—is the backup and recovery unit.

See `procedures/{cd,deployment,ha-service}.md` in the context repo.
