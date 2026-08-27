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
