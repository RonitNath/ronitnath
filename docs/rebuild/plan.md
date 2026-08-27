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
operator (`/platform`, audience of one today). Recurring jobs, all on the
observation lane (`kernel::observe`, started by `server::observe::spawn`):
the last_seen drain, match scanning, and two bounded expiry sweeps — sessions
and links. There is **no change-feed retention job**, because in this cut the
change feed *is* the audit table: pruning it would delete the record. Retention
by cursor horizon arrives with the commitlog state machine (rung 7), which is
the first thing here that has a log separate from the audit.

Authoritative data and actions = the kernel commands: Register, SignIn,
SignOut, RevokeSession, ActAs, AddFactor, RemoveFactor, VerifyEmail, CreateOrganization,
CreateGroup, Invite (mint link), RevokeLink, ClaimLink, SetRole, RemoveMember, Leave, Share, Revoke,
Transfer, CreateDocument, EditDocument, PublishDocument, ProposeMatch,
ConfirmMatch (self-link merge), RuleMatch (operator merge with evidence),
Split, Disable, Enable — and the OpenID Provider's thirteen (leg O1):
SetHandle, RegisterClient, UpdateClient, RotateClientSecret, DeleteClient,
RotateSigningKey, Authorize, ExchangeCode, RefreshToken, ClientCredentials,
RevokeToken, RevokeConsent, EndSession — and the platform operator's own six
(leg P1): GrantOperator, RevokeOperator, ReAuthenticate, SignInAs,
EndImpersonation, RetireKey — and the deployment's own two (leg P2):
EnableProduct, DisableProduct. Forty-nine in all
(`rn_api::commands::ALL_COMMAND_NAMES`). One transaction each, audit row
inside, typed event appended to the change feed.

Every command's authorisation ends in one function —
`kernel::authority::allows(ctx, want)` — whose last clause is
`platform:* #operator`. Not a middleware: the check stays inside the command,
and what is shared is the sentence "…or an operator" rather than the place it
is asked. `cmd::tests::operator` enumerates `ALL_COMMAND_NAMES` and classifies
every one as gated or as authorised by something that is not a principal (a
password, a bearer token, a client credential, the caller's own session,
proof), so a command added with no operator path fails the build.

Trust boundaries (engineering.md surface taxonomy): `public` (landing, public
pages, `/auth`, `/links/<token>` claim page, ops routes, static assets);
`browser-session` (cookie; `/app`, `/org`, `/platform` shells, `/api/*`);
`recipient/embed` (bearer link — only the claim page and the public RSVP-shaped
command); `openid-provider` (`/oidc/*` and `/.well-known/*`, leg O1 — the
cookie reaches its two decision pages, a bearer *access token* reaches
`/oidc/userinfo` and `/oidc/revoke` and nothing under `/api/*`, and a client
credential reaches its token endpoint); `service-principal` — one route now
accepts one, `client_credentials` at `/oidc/token`, which mints a token whose
principal is a client's own `service` party. What that party may *do* is
whatever relations somebody granted it: being a service confers nothing. A bearer never enters `/api/*`; a
cookie never satisfies a bearer route.

Non-goals for this cut (backlog, absent from code): the hiqlite multi-raft
fork, Zenoh, the commitlog state machine, Loro, zone migration, passkeys and *inbound*
OIDC factors beyond the schema column — this deployment is an OpenID
*Provider* (leg O1, `docs/oidc.md`) and is nobody's relying party —
events/calendar/photos products, `/metrics`.
The fabric seam is a trait (`kernel::feed::Feed`) implemented on hiqlite
listen/notify today so rung 6 swaps transport without touching callers. Email
factors are unique deployment-wide (`factor_email_unique_idx`); source-scoped
uniqueness for imported identities is backlog with the import feature.

Also a non-goal here and rung 7's to own: **a horizon on the change feed**.
`audit` is both the permanent record and the feed every subscription, the
invalidator and the observation lane read forward by `id`, and it has no
retention, no compaction and no bound — correct for a log, wrong for a feed.
Growth is bounded per command and every read is an index seek, so it degrades
in disk rather than in latency, which is why it can wait; what rung 7 owes is a
cursor horizon, the way the kernel report's log group has one.

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
no column, key from config; `link.token`, `session.token`, an authorization code, a client secret and an
access or refresh token are random 256-bit bearer secrets stored as SHA-256.
Id prefixes: `p_ i_ o_ g_ v_ r_ s_ f_ m_ c_ l_` — `c_` is an `oidc_client`,
and a client's public id *is* its `client_id`. Relation vocabulary: `viewer <
commenter < editor` on documents; `member < admin < owner` on organizations and
groups; `contact` (person #contact @group); `operator` (platform:* #operator
@person); `authorized` (oidc_client:X #authorized @person — OpenID consent, so
"has this person agreed" is `check()` and not a second authorisation path).
Nesting in code. `check()` is one indexed query over the expanded
subject set. Groups never own; persons and organizations own; `Transfer` is
the only *command* that changes an owner — a merge moves
`resource.owner_party_id` and the `#owner` row onto the survivor, and `Split`
moves them back, because there the owner is not changing, the person is. Product tables reference `identity_id`, never
`person_id`. The Provider's five columns that name a person are kernel columns
and say why, in `crates/kernel/tests/merge_properties.rs`: what an OpenID
client is told about is a human, and merging stays cheap because a `sub`
resolves through `person_alias` and a code and a token are bound to a session,
which binds an identity.

**`sub` is not an id.** It is 256 random bits, pairwise by the client's
*owner*, minted once and never rotated — see `docs/oidc.md`. A person's
`handle` (`preferred_username`) is the one human-chosen name in the model:
unique deployment-wide, changeable, and never freed, because a handle that
changed hands is an impersonation waiting to happen.

Migration 7 (`7_authority.sql`) adds what an operator's authority needs to be
accountable: `session.auth_time` (when a password was last presented, moved by
`ReAuthenticate` and by nothing else), `session.impersonated_by_identity_id`
and `impersonation_reason` (NULL for every ordinary session), and
`link.suspended_at` — a link that is asleep because its party is disabled, not
a delete, because `Enable` has to put it back and `RevokeLink` already means
gone for good. And `audit_object (audit_id, kind, id)`, written inside every
command's own transaction, so "every ruling about this row" is a seek rather
than a scan of the change feed. Three session lives, not one: `SESSION_TTL`
14 days, `OPERATOR_SESSION_TTL` 8 hours chosen at `SignIn` and imposed by
`GrantOperator`, `IMPERSONATION_TTL` 30 minutes.

Migration 8 (`8_product.sql`) adds `product (slug, enabled, changed_at,
changed_by)` and nothing else, because **a product is two halves and only one
of them is data**. What a product *is* — its slug, its name, its summary and
the routes it mounts — is the compiled-in catalogue
`rn_kernel::product::CATALOGUE`, so a slug the binary does not carry cannot be
enabled: a typo is a refusal rather than a row nothing reads, which is the
argument `relation::vocabulary` already makes about unregistered kinds. A slug
with **no row is disabled**, so a release that adds a product does not turn it
on across every deployment on upgrade. Disabling writes one row and touches no
product data: the routes go, the data stays.

**A toggle reaches every node through the change feed, and no router is
rebuilt.** Every node holds `AppState::products`, a `ProductSet` — a bitmask
over the catalogue's order — loaded at boot and re-read by the feed consumer
every node already runs (`server::sub::invalidate`) on `ProductEnabled` and
`ProductDisabled` and on nothing else. One `SELECT` per toggle per node, and a
toggle is a human action. The routes themselves are mounted once, at boot, and
wrapped where they merge in one gate (`server::product::gate`) that answers the
uniform `404` while the product is off — a `404` and not a `403`, so a disabled
product is indistinguishable from one this deployment never had. Rebuilding the
`Router` into an `ArcSwap` per toggle was considered and rejected (requirement
B5.4): it buys nothing a caller can observe and costs a whole-router clone plus
an in-flight request holding the old tree with the old per-route state. The
gate reads one atomic word; there is no configuration for it, because the
enablement *is* the configuration and it lives in the database.

`audit.request_digest` is taken over **redacted** arguments
(`kernel::audit::REDACTED_FIELDS`): the audit table is also the change feed and
has no retention, so a password reaching the digest input would be a permanent,
offline-guessable record of it. One consequence is stated rather than
discovered: two calls under one idempotency key that differ *only* in a
credential are a replay rather than a conflict, which is the safer answer for a
retry.

**Impersonation is a real session, not a fourth principal.** `SignInAs` mints a
`session` row for an active identity of the target, so `principal::expand`
resolves the target's own subject set and `check()` learns no new case;
`Principal::Member` carries `impersonated_by` and `cmd::refs::actor` reads it,
which makes "the operator is the actor and the person is the hat" true of every
audit row in the deployment by construction. Ten commands are refused from an
impersonated session — the rule in one sentence is that it may not change what
the person is or who may become them.

Statuses are projections: `party.status ∈ active|disabled|merged`,
`identity.status ∈ active`, `session` has no status (row = live),
`resource.status ∈ draft|published`, `match_candidate.status ∈
proposed|confirmed|rejected`. A status no command produces does not exist.

Two values in migration 1 are admitted by a CHECK constraint and produced by
nothing: `identity.status = 'disabled'` (disabling is a decision about a
person, so `Disable` writes `party.status` and `SignIn` reads it; no command
writes `identity.status` after `Register`) and `resource.status = 'deleted'`
(there is no delete in this cut; six statements filter `<> 'deleted'` and none
writes it). Neither is a status the model has, so both are out of the Rust
enums — `IdentityStatus`, `ResourceStatus` — and `parse` refuses them, which
makes a row carrying one a `RowError` rather than a silently-legal state.
They stay in the schema because migrations are hashed and this file is frozen:
the CHECK admits them until the next fresh formation, which is disposable-dev,
so it is this note and not an `ALTER`. The debt is declared in code as
`Vocabulary::ADMITTED_UNPRODUCED` and the vocabulary tests read it back, so
the day the constraint stops admitting them the declaration fails rather than
rots.

## API (binding)

- `GET /api/whoami` → `{ identity: {public_id, display}, person?: {public_id, display},
  handle?: <preferred_username>,
  acting_as: {kind, public_id, display}, tiers: [member|org|platform],
  organizations: [{public_id, display, role}], session_expires_at }`. Never emails,
  never internal ids, never roles beyond what the chrome renders.
- `POST /api/cmd/<name>` JSON body `{ key: <idempotency uuid>, after?:
  <offset>, ...args }` → `200 { offset, result }` | `409` replayed key with a
  different body | `422 { invalid: [{ field, message }] }` for a malformed
  request, which is the one refusal that may be specific because it describes
  what the caller itself sent | `401` with the uniform decline body for a
  *sensitive* command whose session has not presented a password inside
  `authority::PLATFORM_REAUTH_WINDOW` (15 min) — the second refusal a caller
  may tell apart, and the exception is narrow: the caller is already inside the
  platform tier, so nothing is disclosed, and what they must do is something
  they cannot guess from a uniform decline. The set is named
  (`authority::SENSITIVE`) and is the same seam a second factor would gate |
  `503` uniform decline when this node passed
  its deadline (`http::COMMAND_DEADLINE`) — a timed-out command is *undecided*,
  which is what the key is for | `403/404` uniform decline for everything
  else. `after` is the read-your-writes barrier: the offset the caller has
  already been shown, which this node waits to reach before reading anything.
  A caller that cannot carry an envelope — the `/auth` form posts — sends it
  as `x-rn-after` and reads it back off `x-rn-offset`. Same-origin enforced via
  `Sec-Fetch-Site` (never Origin equality — see memory `no-referrer makes
  Origin null`).
- `GET /api/q/<name>?…` → JSON, `Cache-Control: private, no-store` — the
  response carries session-shaped state, so it is private to one reader and
  not stored, including by the browser's own bfcache. A query handler receives
  a `ReadStore` type that has no `execute` — the "nothing reachable from a GET
  writes" rule is enforced by the type system, and a test asserts `ReadStore`
  exposes no mutating method.
- `WS /api/sub` — client sends `{ subscribe: [<query name + params>], from: <offset> }`;
  server sends `{ offset, query, diff: [{op: put|del, key, row}] }` derived from the
  change feed, plus `{ resync }` after a release mismatch; heartbeat 25 s.
- Auth pages are askama forms posting to `/auth/register`, `/auth/sign-in`,
  `/auth/sign-out`; cookie `rn_session` HttpOnly, Secure in prod, SameSite=Lax.
- **The OpenID Provider** (leg O1, `docs/oidc.md`) is its own surface and its
  own router (`rn_site::oidc::router()`), mountable whole by a downstream
  deployment: `GET /.well-known/openid-configuration` and
  `/.well-known/oauth-authorization-server` (the same document);
  `GET|POST /oidc/authorize` (the cookie, and a consent page); `POST
  /oidc/token` (a client credential, never a cookie); `GET|POST
  /oidc/userinfo` and `POST /oidc/revoke` (a bearer *access token* — the only
  two routes in the deployment that take one, and it reaches nothing under
  `/api/*`); `GET /oidc/jwks` (public, `public, max-age=300`, the only
  cacheable responses on the surface besides assets); `GET|POST
  /oidc/end_session`. Config: `RN_SITE__PUBLIC_ORIGIN` (the issuer, exactly),
  `RN_SITE__OIDC_KEY[_FILE]` (seals the signing keys, and is *not* the id
  key), `RN_SITE__PUBLIC_NAME` (what a page calls this deployment).
- **Impersonation config**, leg P5's to add: `RN_SITE__IMPERSONATION` — whether
  `SignInAs` works at all, default on in dev and **off** in prod. The kernel
  reads it as `Ctx::impersonation`, which the server fills from
  `config.mode == Dev` until the key exists; a deployment that never wants an
  operator able to become a person turns it off rather than trusting a review.
- Queries the Provider adds: `oidc-clients` and `oidc-tokens` (a platform
  operator's; the second is a drill-in on a session) and `authorizations` (the
  reader's own).
- **Products** (leg P2): `platform-products` answers one row per *catalogue*
  entry — slug, display, summary, state, changed at, changed by, and the routes
  it mounts — joined in Rust, because one side of that join is the binary
  rather than a table. A product nobody has decided about is on the screen and
  reads as off. The screen is `/platform/products`; the toggle round-trips
  through `POST /api/cmd/enable-product` | `disable-product` (operator-only and
  *sensitive*) and arrives back as a live diff on `/api/sub`.
  Enabled products mount their own routes on the public surface —
  `starscape` mounts `GET /sky` — and every one of them is behind the gate
  above, so a route added inside a product cannot forget it.
- Shells: `GET /app`, `/org`, `/platform` (+ any sub-path) serve the tier's
  askama shell only if the principal holds that tier; otherwise 404 for
  `/platform`, redirect to `/auth?next=` for the others when anonymous, 404 when
  authenticated-but-unauthorized. `/` is the askama landing, never role-routed.

## Gate manifest

Every row is evidenced in `docs/rebuild/review.md` (leg R1, 2026-08-27): the
exact command that was run, its output, and — where a row could not be run on
the reviewer's machine — why.

| invariant | command | suite | evidence |
|---|---|---|---|
| format / lint | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` | fast | review.md §Acceptance |
| kernel contract | `cargo test -p rn-kernel` (unit + proptest: check nesting, merge idempotence, no group owner, no weak-signal merge, id round-trip/forgery rejection) | fast | review.md §Acceptance, §Model conformance |
| no full scan | `cargo test -p rn-kernel --test explain` — every hot query under `EXPLAIN QUERY PLAN` asserts its index | fast | review.md §Acceptance |
| route/auth matrix | `cargo test -p rn-site` — every route × {anonymous, member, org operator, platform operator, authenticated-unauthorized, bearer} | fast | review.md §Trust boundaries |
| negative space | `cargo test -p rn-site negative_space` — `/manage`, `/api/realtime`, `/pkg/*`, `/metrics`, `/dev-dashboard` are 404 for anonymous *and* signed-in; no `manage`/`capability`/`resource_grants` symbol in the tree (`rg` in a test, scoped to `crates/server/src` — the tree-wide check is review.md's) | fast | review.md §Negative space |
| GET never writes | type-level (`ReadStore`) + `cargo test -p rn-site read_store_has_no_execute` | fast | review.md §Negative space |
| bundles build | `trunk build --release` for each bundle; `cargo check --target wasm32-unknown-unknown -p rn-api -p rn-ui -p rn-app -p rn-org -p rn-platform -p rn-starscape` | fast | review.md §Acceptance |
| size gates | `tools/size-gate.sh` (≤200 root, warn 350, fail 600) | fast | review.md §Size gates |
| the OpenID Provider | `cargo test -p rn-site --test oidc` — an in-repo relying party through the whole flow (discovery → authorize → code → token → id token validated against the JWKS → userinfo → refresh → revoke → sign-out with a back-channel receipt), plus the negative space and a `private_key_jwt` replay | fast | review.md §OpenID Provider |
| the Provider, against a real RP | `tools/oidc-rp.sh` — `oauth2-proxy` against `just run-dev`, clicked through by hand | release | `docs/review/o1/` |
| golden flows | `end2end/` playwright, `--workers=1 --project=chromium` against a node seeded by `tools/seed.sh`: register → org → group → invite → claim (second browser) → share doc → live diff arrives; merge two self-registered identities; operator ruling; revoke session kills the other tab | release | review.md §Golden flows |
| visual | agent-browser screenshots of every SPA route and askama page, both themes, 1440 and 390 wide, viewed | release | review.md §Visual conformance |
| performance | `cargo bench -p rn-kernel` against budgets in the kernel report; `tools/perf/run.sh` (oha + samply on a 3-node local cluster) → `docs/perf/<date>.md` | release | `docs/perf/2026-08-27.md`, `docs/perf/2026-08-27-f5.md`; verified against their raw evidence in review.md §Performance |
| resilience | `tools/cluster.sh` 3 nodes; kill one, commits continue; kill two, writes refuse with the uniform decline (`503`, bounded by `http::COMMAND_DEADLINE`); restart, converges | release | `docs/perf/2026-08-27-f5/drill-{1,2}.txt`; read in review.md §Resilience |
| release build | Containerfile builds; `/readyz` reports version; CI workflow runs the fast suite | release | review.md §Release build — not runnable in the review worktree |

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
