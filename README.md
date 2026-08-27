# rn-site

ronitnath.com. An axum server that renders the public pages with askama and
hands each signed-in audience its own Leptos CSR bundle, over a kernel that
owns the model — identities, people, organizations, groups, relations,
resources and the commands that change them. Three replicas on nexus, nyc and
delenda hold the data themselves in a three-voter hiqlite cluster.

`docs/rebuild/plan.md` is the binding contract: product surface, API shape,
model, gate manifest. `docs/kernel/index.html` is the ratified model and
`docs/design.md` the design authority.

## Tiers

Four audiences, and the boundary between them is the whole authorisation
model. A route names the extractor it needs, and that extractor *is* the
authorisation — there is no request-level guard a new route could forget.

| tier | who | surface |
|---|---|---|
| visitor | anonymous | `/` (the landing and its starscape), public pages, `/auth`, `/links/<token>` |
| member | any signed-in person | `/app` — their identities, sessions, organizations, documents |
| organization operator | `admin`/`owner` in an organization | `/org` — that organization's members, groups, invitations, shares |
| platform operator | `platform:* #operator` | `/platform` — parties, identities, matches, sessions, audit, cluster |

`/platform` 404s for anyone who does not hold it; `/app` and `/org` redirect an
anonymous visitor to `/auth?next=` and 404 an authenticated one who does not.
A bearer link never reaches `/api/*`, and a cookie never satisfies a bearer
route.

## The workspace

```
crates/kernel        rn-kernel      the model: schema + migrations, store, ids, principal + check(),
                                    commands + audit + change feed, merge, observation lane
crates/server        rn-site        axum: askama pages, /api/cmd|q|sub|whoami, tier shells, ops
                                    routes, config, cluster bootstrap, assets
crates/api           rn-api         wire types, shared by the server and every bundle
crates/ui            rn-ui          shared Leptos CSR pieces + tokens.css, the one stylesheet
crates/app-member    rn-app         the /app bundle
crates/app-org       rn-org         the /org bundle
crates/app-platform  rn-platform    the /platform bundle
crates/starscape     rn-starscape   the landing's sky
templates/           askama templates          static/    assets served as-is
tools/               cluster, seed, size gate, star catalog
end2end/             playwright golden flows   docs/      plan, kernel, design, perf
```

Release builds embed each bundle's `dist/` and the `static/` tree into the
binary with rust-embed, so a node serves its whole frontend out of one file.
The one exception is `static/stars/lod` — 49 MB of regional Gaia catalog, read
from `RN_SITE__STATIC_DIR` by the byte-range handler and shipped in the image
rather than compiled into it.

## Working on it

```sh
cargo run -p rn-site                    # the server on :3004, dev mode, disk assets
cp .env.example .env                    # then RN_SITE__DEV=1 puts a "Sign in as
                                        # operator" button under the sign-in form
                                        # (debug builds only; `just run-dev`)
tools/seed.sh                           # register the first operator and grant it
just up                                 # this worktree's own instance
tools/cluster.sh start                  # three real voters on this host
tools/perf/run.sh                       # oha + samply against that cluster
tools/size-gate.sh                      # the structure limits, as CI runs them
```

`just` fronts all of these — `just gate` is the CI gate in order, `just --list`
the rest (bundles, e2e, cluster, perf, drill, image).

Dev mode reads the bundles and the static tree from disk, so a stylesheet edit
is a reload. Building a bundle at all takes trunk:

```sh
trunk build --release --config crates/app-member/Trunk.toml
```

That `--config` form is the only one that works from the workspace root —
trunk resolves its configuration from the working directory or from `--config`
and never from the path of the index it is handed. For live work on a bundle,
`crates/ui/README.md` has the two-terminal recipe (the fixture API on :3199 and
`trunk serve` at the tier path).

`just up` is the per-worktree instance (`tools/ephemeral.sh`): it derives a
slot from the worktree's absolute path — `sha256(path) mod 100` — and takes
its three ports from it (`3300 + slot` for HTTP, `8300 + slot` and
`8400 + slot` for hiqlite's two listeners), keeping its database, log, pid
file, id key and signing key under `<worktree>/target/ephemeral/`. So two
checkouts, or two agents, are up at the same time without knowing about each
other, and neither collides with `cargo run` on :3004 or `tools/cluster.sh` on
3161-3163. It runs in dev mode with the developer sign-in on, so the button
under the sign-in form is a working operator session; `just eph status` says
where it is, `just down` stops it and `just eph reset` wipes the state.
Nothing it writes is committed — `target/` is ignored.

The first operator is not a migration and not a seed row: `tools/seed.sh`
registers one through the real form, then runs `rn-site bootstrap-operator
<email>` against the stopped server — the kernel refuses a second one. That
subcommand needs the database to itself, which a formed cluster never gives it;
on a deployment the running process takes the same grant instead, from
`RN_SITE__BOOTSTRAP_OPERATOR_EMAIL` (`deploy/CUTOVER.md` §6).

## Delivery

Tier 1 (`procedures/delivery.md`): `main` is where work lands, `deploy` is
where it ships, and a push to `deploy` runs gates on the zero-secret runner,
publishes an immutable digest, and rolls it across the three nodes with a
health gate at each. `docs/README-ops.md` has the ops surface, the release
path, provisioning and the failure modes; `deploy/CUTOVER.md` is the
fresh-formation runbook.

**Cutover note.** This rebuild does not migrate the live cluster's state — it
archives it. The data lifecycle is disposable-dev by ruling
(`docs/rebuild/plan.md`), the schema is new, and the first release forms an
empty three-voter cluster from nothing. Every account, session and link on the
old deployment ends at that moment, deliberately.
