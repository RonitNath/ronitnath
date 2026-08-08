# ronitnath.com Site

Ronit Nath's personal site. Rust + Leptos (islands mode) + Axum, one binary,
served from three replicas behind the public edge.

The landing page is ronitnath.com. Options:
- Authenticate

## `/auth`

`/auth` is the login/register page, and **it declines every submission.**

There is no identity backend behind it yet, so `POST /api/authenticate` reads
nothing from what it is given and answers every input identically:
`Authentication failed.` — no field-level hint, no distinction between an
unknown address and a wrong password, no session, no cookie, no redirect. The
decline is content rather than a transport error (HTTP 200), so the page keeps
working and the route reads as a gate that declined you rather than one that is
broken. The route stays visible on purpose.

The auth model below is the shape this will grow into; none of it is built.

Auth model:
- Auth Factors
    - Referential:
        - Email
    - Providence
        - Password
    - External
        - OIDC/Oauth2
- Identity
    - Who literally is this person?
    - What kind of entity are they?
- Accounts
    - Personal, Business, Shared
    - Legal ownership boundary
- Access
    - What capabilities does this person have?
- Session
    - Where is this account authorized to act currently?

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
cargo leptos watch     # http://127.0.0.1:3000
```

The gates that CI enforces, runnable locally:

```sh
cargo fmt --check
cargo clippy --no-default-features --features ssr --all-targets -- -D warnings
cargo test --locked --no-default-features --features ssr
```

The crate has no default features: `ssr` is the server build and `hydrate` is the
wasm one, so every command names the feature set it means.

## Deployment

Push to `deploy` runs Forgejo Actions: gates on the zero-secret native runner,
then a privileged release job that builds `Containerfile`, publishes an immutable
digest to the forge registry, and rolls it across **nexus, nyc and delenda** with
the fleet `rn-site-deploy` playbook. `deploy/provision.sh` prepares the hosts and
is deliberately *not* part of CD — an operator provisions, the pipeline only ever
rewrites a digest.

See `procedures/{cd,deployment,ha-service}.md` in the context repo.
