# rn-site rebuild — contract and plan (2026-08-26)

Owner ruling 2026-08-26: nuke the frontend and rebuild it as pure Leptos CSR
bundles per capability tier plus askama static pages; rewrite all server logic
on the ratified kernel model (`docs/kernel/index.html` — the prose;
`docs/model-board.tldr` — the diagrams). Nothing legacy survives. Proper
testing, benchmarking and profiling are part of the deliverable, not a
follow-up. Every worker reads this file, then the kernel report, then only the
files its brief names.

## Product contract

Users: a visitor (anonymous, reads the landing and public pages); a member
(any signed-in person: `/app`); an organization operator (`/org`); a platform
operator (`/platform`, audience of one today). Recurring jobs: session expiry
sweep, observation drain (last_seen, match scanning), change-feed retention.

Authoritative data and actions = the kernel commands: Register, SignIn,
SignOut, RevokeSession, AddFactor, RemoveFactor, VerifyEmail, CreateOrganization,
CreateGroup, Invite (mint link), ClaimLink, SetRole, Leave, Share, Revoke,
Transfer, CreateDocument, EditDocument, PublishDocument, ProposeMatch,
ConfirmMatch (self-link merge), RuleMatch (operator merge with evidence),
Split, Disable, Enable. One transaction each, audit row inside, typed event
appended to the change feed.

Trust boundaries (engineering.md surface taxonomy): `public` (landing, public
pages, `/auth`, `/links/<token>` claim page, ops routes, static assets);
`browser-session` (cookie; `/app`, `/org`, `/platform` shells, `/api/*`);
`recipient/embed` (bearer link — only the claim page and the public RSVP-shaped
command); `service-principal` — none in this cut (the `service` party kind
exists in the schema, no route accepts it). A bearer never enters `/api/*`; a
cookie never satisfies a bearer route.

Non-goals for this cut (backlog, absent from code): the hiqlite multi-raft
fork, Zenoh, the commitlog state machine, Loro, zone migration, passkeys/OIDC
factors beyond the schema column, events/calendar/photos products, `/metrics`.
The fabric seam is a trait (`kernel::feed::Feed`) implemented on hiqlite
listen/notify today so rung 6 swaps transport without touching callers. Email
factors are unique deployment-wide (`factor_email_unique_idx`); source-scoped
uniqueness for imported identities is backlog with the import feature.

Data lifecycle: **disposable-dev**. The live cluster's state is archived, not
migrated, at cutover (that cutover is a separate owner decision — see §Release).

## KDR(D)

| behavior | verdict |
|---|---|
| identity/account/membership/capability/grant model, `/auth` SSR pages, `/manage`, `/api/realtime` invalidation hub, Leptos islands SSR, tailwind, `dev-dashboard`, dev bypass, `tests/*`, `end2end/*` | **Delete** |
| hiqlite cluster bootstrap (`operations/db.rs` node config, secrets, migration integrity), config, shutdown, telemetry, ops routes (`/healthz /readyz /version`), browser-freshness headers (`X-RN-App-Version`, no-store/no-cache split) | **Replace** — rewritten into `crates/server`, same contract, re-reviewed; no file is copied verbatim |
| starscape renderer (WebGL shaders, star/LOD assets, cities, track, tuning) and `public/stars/*`, `public/js/*` | **Replace** — re-homed as the `starscape` wasm bundle mounted by the askama landing; interaction contract from `end2end/tests/starscape.spec.ts` is re-proven by the new e2e |
| `deploy/*`, `.forgejo/workflows/deploy.yml`, Containerfile, flake | **Replace** — same pipeline shape, new build steps (trunk bundles + server binary) |
| OKLCH tokens and design brief (`docs/design.md`) | **Keep** (design authority; tokens move to `crates/ui/tokens.css`) |

## Workspace

```
Cargo.toml                 workspace; [workspace.dependencies] pins every version once
crates/api        rn-api      wire types only (serde), wasm-safe, no_std-free; shared by server and bundles
crates/kernel     rn-kernel   the model: ids, schema+migrations, store (hiqlite + rusqlite for tests/benches),
                              party/identity/factor/person/session/link, resource registry, relation store,
                              principal + check(), commands + audit + feed, observation lane. Tests, proptests,
                              EXPLAIN tests, criterion benches live here.
crates/server     rn-site     axum: askama pages, /api/cmd|q|sub|whoami, shell-per-tier, static bundles,
                              ops routes, config, telemetry, cluster bootstrap. Route-matrix tests live here.
crates/ui         rn-ui       shared Leptos CSR components: shell (rail/drawer), table (sort/filter/paginate/
                              priority-elided columns), form primitives with immediate sync, api client,
                              subscription store; tokens.css
crates/app-member rn-app      /app bundle (trunk)
crates/app-org    rn-org      /org bundle (trunk)
crates/app-platform rn-platform  /platform bundle (trunk)
crates/starscape  rn-starscape  landing-page wasm bundle (trunk)
templates/        askama templates (landing, public pages, auth, claim, shell)
static/           assets served as-is (stars, fonts, tokens)
tools/            perf harness (oha profiles, samply, 3-node cluster script), star catalog builder
end2end/          playwright golden flows
docs/             design.md, kernel/, rebuild/, perf/<date>.md
```

Versions (verified on crates.io 2026-08-26): leptos **0.8 latest stable**
(0.9 is still beta; CSR-only has no reason to ride a beta), leptos_router 0.8,
hiqlite 0.14, axum 0.8, askama 0.16, trunk 0.21 (installed), criterion 0.8,
proptest 1.11, rust-embed 8. Rust 1.97.1 (flake pin). Bundles build with
`trunk build --release` into `crates/<bundle>/dist`, embedded by the server
via rust-embed at build time (release) or served from disk (dev).

Structure rules (engineering.md §Structure): feature modules own handlers,
templates, tests; root files ≤200 lines; production files warn at 350, fail
at 600 without a written justification; no `utils`/`common` dumping grounds.

## Model (binding — from the kernel report)

Schema exactly as the report's §Schema; ids: internal `id` INTEGER never
serialized, `public_id` = type-prefixed encrypted id (AES-128 of
`table_tag ‖ rowid`, base64url, 22 chars, prefix `p_ i_ o_ g_ r_ …`) derived,
no column, key from config; `link.token` and `session.token` are random
256-bit bearer secrets stored as SHA-256. Relation vocabulary: `viewer <
commenter < editor` on documents; `member < admin < owner` on organizations and
groups; `contact` (person #contact @group); `operator` (platform:* #operator
@person). Nesting in code. `check()` is one indexed query over the expanded
subject set. Groups never own; persons and organizations own; `Transfer` is
the only *command* that changes an owner — a merge moves
`resource.owner_party_id` and the `#owner` row onto the survivor, and `Split`
moves them back, because there the owner is not changing, the person is. Product tables reference `identity_id`, never
`person_id`.

Statuses are projections: `party.status ∈ active|disabled|merged`,
`identity.status ∈ active|disabled`, `session` has no status (row = live),
`resource.status ∈ draft|published|deleted`, `match_candidate.status ∈
proposed|confirmed|rejected`. A status no command produces does not exist.

## API (binding)

- `GET /api/whoami` → `{ identity: {public_id, display}, person?: {public_id, display},
  acting_as: {kind, public_id, display}, tiers: [member|org|platform],
  organizations: [{public_id, display, role}], session_expires_at }`. Never emails,
  never internal ids, never roles beyond what the chrome renders.
- `POST /api/cmd/<name>` JSON body `{ key: <idempotency uuid>, ...args }` →
  `200 { offset, result }` | `409` replayed key with a different body |
  `403/404` uniform decline. Same-origin enforced via `Sec-Fetch-Site`
  (never Origin equality — see memory `no-referrer makes Origin null`).
- `GET /api/q/<name>?…` → JSON; cacheable `private, no-store`; a query handler
  receives a `ReadStore` type that has no `execute` — the "nothing reachable
  from a GET writes" rule is enforced by the type system, and a test asserts
  `ReadStore` exposes no mutating method.
- `WS /api/sub` — client sends `{ subscribe: [<query name + params>], from: <offset> }`;
  server sends `{ offset, query, diff: [{op: put|del, key, row}] }` derived from the
  change feed, plus `{ resync }` after a release mismatch; heartbeat 25 s.
- Auth pages are askama forms posting to `/auth/register`, `/auth/sign-in`,
  `/auth/sign-out`; cookie `rn_session` HttpOnly, Secure in prod, SameSite=Lax.
- Shells: `GET /app`, `/org`, `/platform` (+ any sub-path) serve the tier's
  askama shell only if the principal holds that tier; otherwise 404 for
  `/platform`, redirect to `/auth?next=` for the others when anonymous, 404 when
  authenticated-but-unauthorized. `/` is the askama landing, never role-routed.

## Gate manifest

| invariant | command | suite |
|---|---|---|
| format / lint | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` | fast |
| kernel contract | `cargo test -p rn-kernel` (unit + proptest: check nesting, merge idempotence, no group owner, no weak-signal merge, id round-trip/forgery rejection) | fast |
| no full scan | `cargo test -p rn-kernel explain_` — every hot query under `EXPLAIN QUERY PLAN` asserts its index | fast |
| route/auth matrix | `cargo test -p rn-site` — every route × {anonymous, member, org operator, platform operator, authenticated-unauthorized, bearer} | fast |
| negative space | `cargo test -p rn-site negative_space` — `/manage`, `/api/realtime`, `/pkg/*`, `/auth/*` GET-only SSR islands are 404; no `manage`/`capability`/`resource_grants` symbol in the tree (`rg` in a test) | fast |
| GET never writes | type-level (`ReadStore`) + `cargo test -p rn-site read_store_has_no_execute` | fast |
| bundles build | `trunk build --release` for each bundle; `cargo check --target wasm32-unknown-unknown -p rn-ui -p rn-app -p rn-org -p rn-platform -p rn-starscape` | fast |
| size gates | `tools/size-gate.sh` (≤200 root, warn 350, fail 600) | fast |
| golden flows | `end2end/` playwright: register → org → group → invite → claim (second browser) → share doc → live diff arrives; merge two self-registered identities; operator ruling; revoke session kills the other tab | release |
| visual | agent-browser screenshots of every SPA route and askama page, both themes, 1440 and 390 wide, viewed | release |
| performance | `cargo bench -p rn-kernel` against budgets in the kernel report; `tools/perf/run.sh` (oha + samply on a 3-node local cluster) → `docs/perf/<date>.md` | release |
| resilience | `tools/cluster.sh` 3 nodes; kill one, commits continue; kill two, writes refuse; restart, converges | release |
| release build | Containerfile builds; `/readyz` reports version; CI workflow runs the fast suite | release |

## Ladder — bounded legs

Each leg is one worker in its own worktree on branch `rebuild/<leg>`, merged
into `rebuild` by the orchestrator. A leg owns the paths it names and nothing
else.

| leg | owns | depends on | end state |
|---|---|---|---|
| K0 skeleton | workspace, `crates/api`, empty crates, `tools/size-gate.sh`, deletion of legacy tree | — | workspace builds (`cargo check --workspace`), old code gone, api types match §API |
| K1 kernel core | `crates/kernel` | K0 | schema, ids, store, party/identity/factor/session/link, Register/SignIn/SignOut/Revoke/AddFactor/VerifyEmail, principal expansion, audit, feed trait + hiqlite impl, benches for principal + id |
| S1 server + presence | `crates/server`, `crates/starscape`, `templates/`, `static/`, `tools/cluster.sh` | K0 | binary serves landing with starscape, ops routes, config, cluster bootstrap, static; 3-node local script |
| U1 ui + bundles | `crates/ui`, `crates/app-*` | K0 | shell, table, forms, api client, sub store; three bundles build and render chrome from a fixture whoami |
| K2 relations | `crates/kernel` (relation, resource, org/group/membership/invitation, document, Share/Revoke/Transfer, check) | K1 | check() benches + EXPLAIN tests green under budget |
| K3 merge | `crates/kernel` (match, person_link, alias, ConfirmMatch/RuleMatch/Split, observation lane) | K1 | merge proptests + bench |
| S2 api | `crates/server` (`/api/*`, auth pages, claim page, shells, route matrix, negative space) | K1, S1 | full matrix green |
| U2/U3/U4 | one bundle each, wired to real API | K2, K3, S2 | every feature reachable in the browser, e2e per tier |
| P1 perf | `tools/perf`, `docs/perf` | K2, S2 | measured numbers vs budgets, flame graphs, drill results |
| R1 review | read-only adversarial pass, then fixes routed to owners | all | manifest rows all evidenced |

## Release

`main` ← `rebuild` once the fast suite, golden flows and perf report are
green. Shipping (push to `deploy`) requires a fresh-formation cutover of the
hiqlite state (archive first) — an owner decision, taken separately.
