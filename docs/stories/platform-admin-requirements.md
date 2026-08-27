# Platform admin — feature requirements (2026-08-27)

The input is `docs/stories/platform-admin.md`: 21 stories and the rulings that
shape them. This file says, for each one, what the tree does today with the
evidence, what is missing, and the requirements that close it — commands,
queries, routes, screens, model changes, config keys — each marked

| mark | meaning |
|---|---|
| **EXISTS** | shipped and story-complete; the acceptance test is the regression |
| **EXTEND** | the mechanism is there and does not reach far enough |
| **NEW** | nothing in the tree does this |
| **DEFER** | out of this arc, with the reason and what unblocks it |

Every requirement carries one falsifiable acceptance test. "Falsifiable" means
a command a reviewer can run whose failure is a sentence, not a judgement.

Diagrams: `docs/design/platform-admin/` (`dist/board.html`) — the frames are
named in the requirements that they draw. Implementation legs:
`docs/stories/platform-admin-legs.md`.

The OpenID Provider (`crates/kernel/src/oidc/**`, `crates/kernel/src/cmd/oidc/**`,
migration `6_oidc.sql`) is leg **O1**, in flight in the main checkout. Stories
E14 and E15 are its delivery; what appears below for them is the operator
*surface* over what it built, and nothing that redoes it.

---

## Findings that cut across the stories

Five things came out of the reading that are not one story's business, and
every leg below inherits them.

**F1. God mode is eight commands wide, not twenty-six.**
`is_platform_operator` (`crates/kernel/src/cmd/mod.rs:363`) is consulted by
`disable`, `enable` (through `disable::authorised`), `revoke`, `revoke_link`,
`revoke_session`, `transfer`, `rule_match`/`split`, and the OIDC commands
(`crates/kernel/src/cmd/oidc/mod.rs`). The other eighteen command modules
authorise against ownership or membership alone — so an operator cannot
`set_role` inside somebody's organization, cannot `remove_member`, cannot
`invite`, cannot `edit_document`, cannot `share`. The ruling says *full
authority over every party and resource*. That gap is requirement **D12.2**
and it is the single largest piece of work in the arc.

**F2. `SetRole` cannot grant the operator relation, and plan.md says it can.**
`crates/kernel/src/cmd/mod.rs:361` reads "The row is written by an operator
seeding it or by K2's `SetRole`". `set_role` (`crates/kernel/src/cmd/set_role.rs:33`)
writes `membership` through `org::SET_ROLE_SQL` and takes a *container*; the
operator relation is a `relation` row on `platform:0` and `platform` is not a
container (`crates/kernel/src/relation/kinds.rs:124`, `is_container: false`).
So today the only way to become an operator is `bootstrap_operator`, which
refuses the second one (`crates/kernel/src/merge/rule.rs:190`). **A2 is
unimplementable as documented**; requirement A2.1 replaces the comment.

**F3. A disable deletes what an enable cannot restore.**
`disable` deletes sessions, OIDC tokens and codes
(`crates/kernel/src/cmd/disable.rs:85-87`); `enable`
(`crates/kernel/src/cmd/enable.rs:36`) flips `party.status` and nothing else.
For sessions and tokens that is correct — they are re-mintable by signing in.
For **links** it is not done at all: a disabled person's outstanding invitation
links keep working, because nothing in `disable` touches `link` or the
`group:X #member @link:T` rows. Requirement C9.2.

**F4. An operator's ruling is invisible to the person it was about.**
`Named::Audit` (`crates/server/src/api/query/named.rs:41`) is "the acting
identity's own audit rows", keyed by actor. An operator revoking your session
writes a row whose actor is the operator, so it is not in your list. Story 10
says "the owner can see that I did". Requirement C10.2.

**F5. There is no product concept in the tree at all.**
`rg -n 'product' crates/` finds only prose. Runtime product enablement (story
5) is therefore entirely new model, and it is the one requirement with a
propagation problem, because axum builds its `Router` once at boot
(`crates/server/src/lib.rs:47`). The design is B5 below and frame **A** of the
board.

---

## A. Becoming and staying the operator

### A1 — the first operator is configured ahead of time

**Today.** Two ways in, both real. `rn-site bootstrap-operator <email>` opens
the database directly (`crates/server/src/main.rs:71`,
`crates/server/src/bootstrap.rs:66`), and
`RN_SITE__BOOTSTRAP_OPERATOR_EMAIL` (`crates/server/src/config.rs:59`) has the
serving process take the same grant at boot and on every `Registered` event
(`crates/server/src/bootstrap.rs:139`). The kernel refuses a second
(`crates/kernel/src/merge/rule.rs:190`, `ANY_OPERATOR_SQL`).
`tools/seed.sh` drives the first form for a laptop.

**Missing.** Nothing for ronitnath. For isoastra the story continues into A2,
which does not exist.

**Requirements.**

- **A1.1 — EXISTS.** The configured-address bootstrap, unaudited, once.
  *Acceptance:* `cargo test -p rn-site bootstrap` — a second
  `RN_SITE__BOOTSTRAP_OPERATOR_EMAIL` grant on a deployment that has an
  operator writes no `relation` row and the count stays 1.
- **A1.2 — EXTEND.** The bootstrap is silent about having happened. It writes
  no audit row by design (`rule.rs:177`) and that is right, but the
  *deployment screen* must state it: "operator since <t>, granted by
  configuration". Read from `relation.at` where `granted_by IS NULL`.
  *Acceptance:* a fresh ephemeral instance, bootstrapped, renders that line on
  `/platform/operators` and the row's `granted_by` is null.

### A2 — delegate operator authority and take it back

**Today.** Nothing. See finding **F2**: `SetRole` cannot write this row, and
`bootstrap_operator` declines once one exists. The relation vocabulary already
admits it — `platform #operator @person`
(`crates/kernel/src/relation/kinds.rs:124-132`) — and `check()` reads it
(`crates/kernel/src/cmd/mod.rs:363`).

**Requirements.**

- **A2.1 — NEW. Commands `GrantOperator` and `RevokeOperator`.**
  - *Actor rule:* the caller holds `platform:* #operator`. Not the bootstrap
    path: this one has an actor, so it is a command.
  - *Arguments:* `{ person: PublicId, reason: String }` — `reason` mandatory
    and bounded like `RuleMatch`'s evidence (`EVIDENCE_LIMIT`, 2 000 chars,
    `crates/kernel/src/merge/rule.rs:30`), because a delegation of god mode
    with no stated reason is a row nobody can account for later.
  - *Refusals:* `RevokeOperator` declines when it would leave the deployment
    with none — the same shape as the last-owner rule
    (`crates/kernel/src/cmd/set_role.rs:56`). Revoking yourself is allowed
    while another operator exists.
  - *Audit shape:* `command = 'grant-operator' | 'revoke-operator'`,
    `actor_identity_id` = the granting operator, `acting_as` = their person,
    payload `{event, person, reason}`.
  - *Events:* `OperatorGranted { person }`, `OperatorRevoked { person }`.
    Both must evict the principal cache for that person's sessions
    (`crates/server/src/sub/invalidate.rs`) or a revoked operator keeps the
    tier until their cache entry ages out.
  - *Model:* `relation.granted_by` carries the granting person — the column
    exists and the bootstrap writes NULL into it.
  - *Query:* `platform-operators` — person, display, granted_by (display),
    granted at, whether it was the bootstrap.
  - *Screen:* `/platform/operators`, a table plus a grant control that takes a
    person (search by handle/email/id) and a reason.
  - *Acceptance:* two-person test in `crates/kernel/src/cmd/tests/` — B is not
    an operator, A grants, B's `is_platform_operator` is true and `whoami`
    reports the `platform` tier; A revokes, B's next request 404s on
    `/platform`; A revokes the last operator and the command declines with the
    deployment still holding one.

### A3 — short operator sessions and re-authentication

**Today.** One TTL for everybody: `SESSION_TTL = 14 days`
(`crates/kernel/src/domain/session.rs:19`), renewed past half-life
(`RENEW_AFTER`, ibid:23). The `session` table has no `auth_time`
(`crates/kernel/migrations/1_kernel.sql:142-155`), so nothing can say how long
ago a password was actually presented.

**Requirements.**

- **A3.1 — EXTEND. A shorter life for an operator's session.**
  `OPERATOR_SESSION_TTL` (8 h, one working day) chosen at `SignIn` when the
  person holds the operator relation, and at `GrantOperator` for that person's
  live sessions — a promotion must shorten what it promotes, or the grant
  hands out a 14-day god-mode cookie.
  *Acceptance:* a kernel test signs in an operator and a member on the same
  clock and asserts the two `expires_at` differ by exactly
  `SESSION_TTL - OPERATOR_SESSION_TTL`.
- **A3.2 — NEW. `session.auth_time` and a re-auth window for platform
  commands.** Migration column `auth_time INTEGER NOT NULL`, written by
  `SignIn` and by a new `ReAuthenticate` command (password re-presented, no
  new session row). A named set of *sensitive* commands — `GrantOperator`,
  `RevokeOperator`, `SignInAs`, `Disable` on a person, `RotateSigningKey`,
  `DeleteClient`, `EnableProduct`/`DisableProduct`, `Restore` — declines when
  `now - auth_time > PLATFORM_REAUTH_WINDOW` (15 min). The decline is
  distinguishable, unlike every other one: `401` with the same shape `422`
  uses, because a caller that must re-authenticate has to be told, and the
  caller is already inside the tier.
  *Acceptance:* a kernel test runs `GrantOperator` on a session whose
  `auth_time` is 16 minutes old and gets the re-auth refusal; runs
  `ReAuthenticate` and the same command succeeds. A server test asserts the
  status is 401 and that no non-sensitive command answers 401.
- **A3.3 — DEFER. Mandatory second factor for operators.**
  Passkeys are a declared non-goal of this cut (`docs/rebuild/plan.md`
  §Non-goals — "passkeys/OIDC factors beyond the schema column"). The story
  says *once passkeys exist*, which is the same fence. Unblocked by a WebAuthn
  factor kind; A3.2's re-auth window is the interim control and is written to
  be the same seam (the sensitive set is what a second factor would gate).

---

## B. Knowing the deployment's state

### B4 — one screen for nodes, versions, rollout, observations, feed, keys

**Today.** `/platform/cluster` (`crates/app-platform/src/cluster.rs:32`) reads
`GET /api/q/cluster` (`crates/server/src/api/cluster.rs:50`) on a 5 s timer and
renders `ClusterView` (`crates/api/src/cluster.rs:60-78`): node, version, both
raft groups with `voters`/`expected_voters`/`leader`, feed head, subscriber
count, pending observations. `/readyz` reports the same two groups
(`crates/server/src/ops.rs:58`).

**Missing.** It is *this node's* account of itself. A three-voter deployment
has three of them and no page shows all three, so "version per node" and
"whether a rollout is mid-way" cannot be answered at all. Key ages are not in
the view.

**Requirements.**

- **B4.1 — NEW. `node_report`, written by the observation lane.**
  Migration: `node_report (node_id INTEGER PRIMARY KEY, node TEXT NOT NULL,
  version TEXT NOT NULL, raft_role TEXT NOT NULL, feed_head INTEGER NOT NULL,
  reported_at INTEGER NOT NULL)`. Each node upserts its own row from the
  observation lane (`crates/server/src/observe.rs`) every 15 s. Not a command
  and not on the feed: it is an observation, which is exactly the lane's
  existing contract (`crates/kernel/src/lib.rs:17`), and putting a heartbeat
  on the change feed would grow the audit table by 5 760 rows a day per node
  in a table that has no retention (`docs/rebuild/plan.md` §Product contract).
  A row older than 3 × the interval renders as *not reporting* — the screen
  says what it witnessed, never what it assumes.
  *Acceptance:* `tools/cluster.sh start`; `GET /api/q/platform-nodes` on node
  1 lists three rows with three node ids; `tools/cluster.sh kill 2`; within
  60 s node 2's row reads *not reporting* on nodes 1 and 3 and the other two
  do not.
- **B4.2 — NEW. Query `platform-nodes` and a rollout verdict.**
  The query returns the `node_report` rows plus one derived field: `rollout`
  ∈ `settled` (one distinct version) | `in progress` (two, with both named) |
  `divergent` (three or more). The word is computed from the rows, so it can
  only say what it counted.
  *Acceptance:* a server test seeds three reports with two versions and
  asserts `rollout = "in progress"` and both version strings present; three
  versions gives `divergent`.
- **B4.3 — EXTEND. The deployment screen.** `/platform` gains a *Deployment*
  page (and `/platform/cluster` becomes a section of it): the node table
  (node, version, raft role, feed head, last report), the two raft groups from
  the existing `ClusterView`, feed head and growth (head now, head 24 h ago —
  from `audit` by `at`), pending observations, subscriber count, and the key
  table from B6. Board frame **A** and UI frame **F1**.
  *Acceptance:* agent-browser screenshots at 1440 and 390, both themes, with
  three voters up and one killed, viewed.
- **B4.4 — EXTEND. Feed growth is a number the system witnessed.**
  Two readings — `max(audit.id)` and `count(*) FROM audit WHERE at > now-86400`
  — labelled *offset* and *commands in the last day*, never extrapolated into a
  rate the process did not measure.
  *Acceptance:* a test asserts the query runs two statements and that both are
  index seeks under `EXPLAIN QUERY PLAN` (`audit` primary key; a
  `audit_at_idx` added by the same migration).

### B5 — products enabled and disabled at runtime, without a redeploy

**Today.** Nothing. There is no `product` table, no product concept in
`crates/kernel`, and the router is built once at boot
(`crates/server/src/lib.rs:47`) from a fixed list of feature routers.

**This is binding** (ruling, `docs/stories/platform-admin.md`): a platform
screen toggles a product and its routes appear or vanish on **every node**
without a restart. Board frame **A** draws the propagation.

**Requirements.**

- **B5.1 — NEW. The catalogue is compiled in; the enablement is data.**
  `rn_kernel::product::Catalogue` — a `&'static [Product]` of `{ slug,
  display, summary, mounts: &'static [&'static str] }`. A product that is not
  in the binary cannot be enabled, so a typo in a slug is a refusal rather
  than a row nothing reads — the same argument
  `crates/kernel/src/relation/vocabulary.rs:277` makes for unregistered kinds.
  Migration: `product (slug TEXT PRIMARY KEY, enabled INTEGER NOT NULL,
  changed_at INTEGER NOT NULL, changed_by INTEGER REFERENCES party (id))`.
  **A slug with no row is disabled.** Default-deny, so a release that adds a
  product does not turn it on across every deployment on upgrade.
  *Acceptance:* a kernel test enables a slug absent from the catalogue and
  gets the uniform decline with no row written.
- **B5.2 — NEW. Commands `EnableProduct` / `DisableProduct`.**
  Operator-only; sensitive (A3.2). Audit `command = 'enable-product'` /
  `'disable-product'`, payload `{event, slug}`. Events `ProductEnabled { slug }`,
  `ProductDisabled { slug }`. Disabling touches no product data — the story
  says *its data stays* — so the command writes exactly one row.
  *Acceptance:* enable, disable, enable again; `SELECT count(*)` on the
  product's own tables is unchanged across all three, and three audit rows
  exist.
- **B5.3 — NEW. The projection, invalidated by the feed.**
  `AppState` holds `products: Arc<ArcSwap<ProductSet>>`, loaded at boot from
  the table and re-read by the existing feed consumer
  (`crates/server/src/sub/invalidate.rs`) on `ProductEnabled|ProductDisabled`
  and on nothing else. Every node runs that consumer already, which is what
  makes this propagate without a broadcast of its own: the change feed *is*
  the fabric (`crates/kernel/src/feed/mod.rs:1`). Bound: one `SELECT` per
  toggle per node, and toggles are a human action.
- **B5.4 — NEW. Routes vanish through one gate, not through a rebuilt router.**
  Each product's routes are mounted at boot and wrapped in a single
  `product_gate(slug)` layer that answers `404` — the uniform decline, not a
  `403`, because a disabled product must be indistinguishable from one this
  deployment never had — when the projection says off. One layer, applied
  where the product's router is merged, so a route added inside a product
  cannot forget it.
  *Rejected:* rebuilding the `axum::Router` into an `ArcSwap` and swapping it
  per toggle. It is possible, and it buys nothing a caller can observe while
  costing a whole-router clone per toggle and an in-flight request holding the
  old tree with the old per-route state. The gate is one comparison against an
  `ArcSwap` load, is testable, and cannot be forgotten.
  *Acceptance — the binding one:* `tools/cluster.sh start` (three voters);
  `curl` the product's route on all three and get 404; `POST
  /api/cmd/enable-product` **on node 1 only**; poll all three and each answers
  200 within 2 s **with no process restarted** (`pgrep` shows the same three
  pids before and after); disable on node 3 and all three return to 404. This
  test is the story, and a leg that cannot produce this transcript has not
  delivered B5.
- **B5.5 — NEW. `whoami.products` and the chrome.** The DTO
  (`crates/server/src/api/whoami/mod.rs`) gains `products: [slug]` — the
  enabled set intersected with what this principal could reach — so a bundle
  does not draw a link to a 404. It renders, so it may be in the DTO
  (`whoami_leaks_nothing`, ibid:8).
  *Acceptance:* `whoami_leaks_nothing` extended; a bundle test asserts the rail
  omits a disabled product's item.
- **B5.6 — NEW. Query `platform-products` and the screen `/platform/products`.**
  Rows: slug, display, enabled, changed at, changed by, and the routes it
  mounts — the last from the catalogue, so the screen states what turning it
  off will actually take away. Toggle inline. Board UI frame **F2**.
  *Acceptance:* screenshots 1440/390 both themes, viewed; the toggle round
  trip goes through `/api/cmd` and the row's live diff arrives on `/api/sub`
  without a reload.
- **B5.7 — DEFER. Per-product settings (including tenant audit visibility,
  story 13).** One table, `product_setting (slug, key, value)`, read the same
  way. Deferred because it has exactly one consumer today (D13.3) and a
  settings store with one setting is a shape guessed rather than observed.
  Unblocked by the second setting.

### B6 — signing keys, rotation with overlap, registered relying parties

**Today.** Leg O1 landed the model: `oidc_key` with
`status ∈ active|retiring|retired`, `kid`, sealed private half, `created_at`,
`retired_at` (`crates/kernel/migrations/6_oidc.sql:251-265`); the
`RotateSigningKey` command retires the active key and mints the next in one
transaction (`crates/kernel/src/cmd/oidc/keys.rs:26-70`); `oidc_client` holds
every registration (ibid:57-91) and `oidc_token` records issuance with
`oidc_token_person_client_idx` (ibid:238).

**Missing.** No operator surface over any of it — no query, no screen — and
nothing ever writes `retired`: `retiring` is terminal in practice, so the JWKS
grows by one key per rotation forever.

**Requirements.**

- **B6.1 — NEW. Query `platform-keys`.** kid, status, created_at, age,
  retired_at, and `signed_tokens_alive` — a count of `oidc_token` rows not
  expired and not revoked that were issued while this key was active. That
  last number is what makes the retire decision safe, and it is a count of
  rows the system holds, not an estimate.
  *Acceptance:* `EXPLAIN QUERY PLAN` on the count asserts
  `oidc_token_expires_idx`; a test rotates twice and asserts one `active`, two
  `retiring`.
- **B6.2 — NEW. Command `RetireKey { kid }`.** `retiring → retired`, operator
  only, sensitive. Declines while `signed_tokens_alive > 0` unless
  `force: true` with a reason, and a forced retire is what makes every token
  under that kid unverifiable — which is a decision, so it lands in the audit
  with its reason.
  *Acceptance:* retire with live tokens declines; the same call with
  `force` succeeds and the JWKS response no longer carries the kid.
- **B6.3 — NEW. Query `platform-clients` and screen `/platform/clients`.**
  Per client: name, owner, redirect URIs, auth method, `trusted`, created,
  secret rotated at, **consents** (count), **last token issued at**
  (`max(oidc_token.created_at)` per client). Rotation, deletion and bulk
  consent revocation are E14/E15's controls on the same screen.
  *Acceptance:* register a client through `/api/cmd/register-client`, exercise
  one token grant, and the screen's *last issued* reads within a second of the
  grant.
- **B6.4 — EXTEND. Key age on the deployment screen (B4.3).** Active key age
  in days beside the node table, and *rotate* as a control there.
  *Acceptance:* covered by B4.3's screenshots.

---

## C. People and identities

### C7 — find any person and see everything the system knows

**Today.** `platform-parties` lists every party and `platform-party` returns
one with its identities, memberships, owned resources and the relations it
holds as a subject (`crates/server/src/api/query/platform_parties.rs:107-111`);
`platform-identities` / `platform-identity` cover registrations and their
sessions and candidates (`crates/server/src/api/query/platform_identities.rs`);
`/platform` renders parties, identities, sessions, audit, matches, resources
and cluster (`crates/app-platform/src/main.rs:32-40`).

**Missing.** From *everything the system knows*: **factors** (kinds, values
masked, verified state), **consents** (`oidc_consent`, landed by O1),
**handles and handle aliases** (`party.handle`, `party_handle_alias`,
`6_oidc.sql:23-35`), **person aliases** from merges, and **search** — the
story says *find any person by handle, email or id* and the parties list is a
paginated table with no lookup.

**Requirements.**

- **C7.1 — EXTEND. `platform-party` gains factors, consents, aliases,
  handle.** Factors masked the way `whoami` masks an address
  (`crates/server/src/api/whoami/name.rs`, `masked`) — an operator page that
  prints every email in the deployment is a page that leaks the deployment if
  a screenshot does. The unmasked value is a per-row control that writes its
  own audit row (`command = 'reveal-factor'`), because reading somebody's
  address is a thing the operator did.
  *Acceptance:* a server test asserts the rendered JSON contains no `@`-bearing
  full address; a second asserts `reveal-factor` writes an audit row naming the
  factor and the operator.
- **C7.2 — NEW. Query `platform-find?q=`.** One box, three answers: exact
  handle, exact email factor, exact public id — never a prefix scan over
  `display_name`, which is the query with no index. Each is a seek
  (`party_handle_unique_idx`, `factor_email_unique_idx`, the id decrypt).
  A miss is empty, not a decline — an operator searching is not being probed.
  *Acceptance:* `cargo test -p rn-site --test explain` covers all three
  statements; a test asserts a 40-character garbage `q` returns `[]` and runs
  no statement at all (it fails the id decrypt first).
- **C7.3 — EXTEND. The person detail screen.** `/platform/parties/<id>`
  becomes the page the story describes: identity, factors, sessions,
  memberships, owned resources, relations held, consents, merge history, and
  the controls (disable/enable, revoke a session, sign in as, transfer,
  grant operator). Board UI frame **F3**.
  *Acceptance:* screenshots 1440/390 both themes, viewed, on an instance
  seeded with a merged person that holds a consent and two sessions.

### C8 — proposed merges, ruling, and splitting a wrong one

**Today.** Complete. `match_candidate` and the observation-lane scanner
(`crates/kernel/src/observe/matches.rs`, `crates/kernel/src/merge/scan.rs`),
`RuleMatch` with mandatory evidence bounded at 2 000 chars
(`crates/kernel/src/merge/rule.rs:30,66-75`), rejection as `rejected` rather
than deletion (ibid:36), `Split` (`crates/kernel/src/merge/split.rs`),
`person_link` carrying method/evidence/asserted_by, queries
`platform-matches`/`platform-match`, and the screen with both commands wired
(`crates/app-platform/src/matches.rs:16`).

**Requirements.**

- **C8.1 — EXISTS.** *Acceptance (regression):* `cargo test -p rn-kernel merge`
  — the proptests in `crates/kernel/src/merge/tests/` already assert merge
  idempotence, no weak-signal merge, and history preserved across a split.
- **C8.2 — EXTEND. The evidence is not on the screen where the ruling is
  made.** `matches.rs` posts `RuleMatch` with an evidence field; the signals
  that produced the candidate (`crates/kernel/src/merge/tests/signals.rs`,
  `merge/candidate.rs`) are what the operator is ruling *on* and belong beside
  it, named — "same verified email", "same handle" — never a score.
  *Acceptance:* the match panel renders one line per signal the candidate row
  actually carries; a test asserts a candidate with one signal renders one
  line, not a percentage.

### C9 — disable a person, an organization or a group; re-enable

**Today.** `Disable` moves `party.status` to `disabled`, deletes the party's
sessions, OIDC tokens and codes, and records the affected RP client ids in its
payload (`crates/kernel/src/cmd/disable.rs:55-91`); `Enable` moves it back
(`crates/kernel/src/cmd/enable.rs:30`). Authorisation covers persons,
organizations and groups (`disable.rs:101-139`). `SignIn` refuses a disabled
party (`crates/kernel/src/dev.rs:39-43` shows the same guard). Nothing is
deleted that the story says must survive.

**Missing.** Links. See finding **F3**: a disabled person's outstanding
invitation links still claim, and their `group:X #member @link:T` rows are
untouched. And the cascade is not visible anywhere — an operator disabling a
person cannot see what went with them.

**Requirements.**

- **C9.1 — EXISTS.** Sessions, tokens, codes; status reversible.
  *Acceptance:* `cargo test -p rn-kernel status` (`cmd/tests/status.rs`).
- **C9.2 — EXTEND. Links die with the party and come back with it.**
  A link is a bearer secret with an expiry and no status
  (`crates/kernel/migrations/1_kernel.sql:166-176`). Add
  `suspended_at INTEGER` — not a delete, because `Enable` has to put it back
  and `revoke_link` already means *gone for good* (`DROP_LINK`,
  `crates/kernel/src/cmd/revoke_link.rs:44`). `Disable` stamps every unclaimed
  link the party minted; `ClaimLink` refuses a stamped one; `Enable` clears the
  stamp. Board frame **D**.
  *Acceptance:* mint a link, disable the minter, claim the link in a second
  browser → the uniform decline; enable, claim → it works. As an e2e spec, not
  only a unit test, because the claim path is a bearer route
  (`crates/server/src/links/mod.rs`).
- **C9.3 — NEW. The cascade is stated before it happens and recorded after.**
  A `platform-cascade?party=` query returns the counts a disable would end —
  sessions, tokens, links, consents — and the confirm control names them.
  After the command, the audit payload carries the same counts.
  *Acceptance:* the counts the preview returned and the counts the audit row
  recorded are equal in a test that disables between the two reads only when
  nothing else moved; where they differ the row is the truth and the preview
  said *at the time of asking*.

### C10 — revoke any session, link or consent, and the owner sees it

**Today.** `RevokeSession` admits the operator
(`crates/kernel/src/cmd/revoke_session.rs:65`) and the platform sessions screen
posts it (`crates/app-platform/src/sessions.rs:18`). `RevokeLink` admits the
operator (`crates/kernel/src/cmd/revoke_link.rs:80`) but there is **no platform
query or screen for links**. `RevokeConsent` exists from O1
(`crates/kernel/src/cmd/mod.rs:66`) with no platform surface.

**Requirements.**

- **C10.1 — EXTEND. Queries `platform-links` and `platform-consents`, and the
  screens.** Links: minted by, container and role granted, expiry, claimed by,
  suspended. Consents: person, client, scopes, granted at. Both with the
  revoke control, and consents with a **bulk** revoke by client (story 15).
  *Acceptance:* revoke a link from `/platform`; the claim page for that token
  answers the uniform decline on the next request; the row leaves the table
  over `/api/sub` without a reload.
- **C10.2 — NEW. Query `about-me` on `/app`, and the finding it closes.**
  Finding **F4**: `Named::Audit` is keyed by actor, so an operator's ruling
  about you is not in your list. `about-me` returns audit rows whose
  `acting_as` is my person **or** whose payload names a `party`/`session`/
  `resource` I hold, whoever the actor was, with the actor's display name. It
  is the story's "the owner can see that I did", and it is also the honest
  half of C11's "the impersonated person's own view shows that it happened".
  *Acceptance:* an operator revokes B's session; B's `GET /api/q/about-me`
  contains that row with the operator named; B's `GET /api/q/audit` does not
  (the two queries are different questions and stay different).
  *Note:* the payload search needs `audit_acting_as_idx` and a JSON extract;
  the extract is not indexable, so the query is `acting_as = me` (a seek)
  UNION the object-keyed rows read from a new `audit_object (audit_id,
  kind, id)` side table written by the same transaction. Without that side
  table this query is a full scan and must not ship.

### C11 — act as any organization, and sign in as any person

**Today.** `ActAs` is **attribution, not authority** by design
(`crates/kernel/src/cmd/act_as.rs:3-8`): it writes `session.acting_as`, which
every audit row carries and no authorisation reads
(`crates/kernel/src/principal/mod.rs` — `expand` does not consult it). It
admits only an organization the person administers (`act_as.rs:63`) or their
own person. There is **no impersonation of a person anywhere in the tree** —
`crates/kernel/src/principal/mod.rs:5` states it as a rule: "The server never
invents a principal of its own — a dev bypass is a fourth kind by another
name". `Principal::Member` has no impersonation field, and the dev sign-in
(`crates/kernel/src/dev.rs:101`) mints an ordinary session for an identity,
which is the closest existing shape.

**Requirements.** Board frames **B** (state machine + sequence) and UI frame
**F5** (the banner).

- **C11.1 — EXTEND. An operator may act as any organization.**
  `act_as.rs:63` gains the operator admission, so the org-scoped audit trail
  reads correctly when an operator works inside a tenant. Attribution only —
  the authority is F1's, and this requirement does not change what any command
  allows.
  *Acceptance:* an operator who administers nothing switches to an
  organization; the next command's audit row carries that party in `acting_as`
  and `check()` returns exactly what it returned before the switch.
- **C11.2 — NEW. Command `SignInAs { person, reason }` — impersonation.**
  - *Shape:* a **new session row** for an active identity of the target
    person, so `principal::expand` resolves the target's real subject set and
    nothing in `check()` learns a new case. The operator's own session is
    untouched and is what they return to.
  - *Model:* `session` gains `impersonated_by_identity_id INTEGER REFERENCES
    identity (id)` (NULL for every ordinary session), `impersonation_reason
    TEXT`, and the row's `expires_at` is `now + IMPERSONATION_TTL` (30 min,
    not `SESSION_TTL`). `Principal::Member` gains `impersonated_by:
    Option<Id<Identity>>`, read by `RESOLVE_SQL`
    (`crates/kernel/src/principal/mod.rs:135`).
  - *The audit rule, and it is one change not twenty-six:* `refs::actor`
    (`crates/kernel/src/cmd/refs.rs`) returns `impersonated_by` as
    `actor_identity_id` when it is set, and the target person as `acting_as`.
    Every command already routes its audit row through it, so *the operator is
    the actor and the person is the hat* everywhere, by construction.
  - *Actor rule:* operator only, sensitive (A3.2), `reason` mandatory and
    bounded. Refused against another operator — an operator who can become
    another operator makes A2's revocation meaningless.
  - *Forbidden while impersonating,* enforced in the dispatcher against
    `Principal::impersonated_by`, each with its own test: `SignInAs`,
    `GrantOperator`, `RevokeOperator`, `AddFactor`, `RemoveFactor`,
    `SetHandle`, `Disable`, `RegisterClient`, `RotateClientSecret`,
    `ReAuthenticate`. The rule in one sentence: an impersonated session may not
    change what the person is or who may become them.
  - *Ends:* `EndImpersonation` (the session is deleted, the operator's own
    cookie is still valid); the 30-minute expiry; the operator losing the
    operator relation — checked at resolve time for impersonated sessions only,
    one extra indexed read on a session kind that is rare; the target being
    disabled (the existing `SignIn`/resolve guard already refuses); the target
    signing out everywhere (`RevokeSession` on it).
  - *Events:* `Impersonated { operator, person }`, `ImpersonationEnded { … }`.
  - *Banner:* `whoami` gains `impersonating: { display, since }` for the
    impersonated session, and the shell (`crates/ui`) draws a persistent bar —
    not a toast, not a pill — that names the person and carries *end*. The
    impersonated person's own record of it is C10.2's `about-me`.
  - *Acceptance:* an e2e spec — operator signs in as B in a second context,
    creates a document, and (a) the document's owner is B, (b) the audit row
    names the operator as actor and B as hat, (c) B's `about-me` contains it,
    (d) `AddFactor` in that session declines, (e) after 30 minutes of clock
    the session resolves to nobody, (f) `EndImpersonation` leaves the
    operator's own session working. Six assertions, one spec.
- **C11.3 — NEW. Impersonation is refused when the deployment says so.**
  `RN_SITE__IMPERSONATION` (default on in dev, **off** in prod until an
  operator turns it on for the session — the product-setting seam of B5.7 when
  that lands). A deployment that never wants it should not have to trust a
  review.
  *Acceptance:* with the key off, `SignInAs` is the uniform decline for an
  operator and the control is absent from the person page.

---

## D. Ownership and rulings

### D12 — transfer anything, revoke any relation

**Today.** `Transfer` and `Revoke` both admit the operator
(`crates/kernel/src/cmd/transfer.rs:86`, `crates/kernel/src/cmd/revoke.rs:41`)
and the resources screen posts both (`crates/app-platform/src/resources.rs:15`).

**Requirements.**

- **D12.1 — EXISTS.** *Acceptance (regression):* `cargo test -p rn-kernel
  sharing` and the resources screen's e2e.
- **D12.2 — EXTEND. God mode across every command (finding F1).**
  One function — `authority::allows(ctx, want) -> Outcome<bool>` — that every
  command's authorisation ends in, admitting `platform:* #operator` as its
  last clause, replacing the eight open-coded `is_platform_operator` calls
  *and* covering the eighteen commands that have none. Not a middleware: the
  check stays inside the command, which is the kernel's stated rule
  (`crates/kernel/src/cmd/mod.rs:8-11`).
  *Acceptance — the one that makes it real:* a table-driven kernel test that,
  for **every** command in `ALL_COMMAND_NAMES`, builds a world where the actor
  holds nothing, asserts the command declines, grants
  `platform:* #operator`, and asserts it now succeeds or declines *for a
  reason that is not authorisation* (the last-owner rule, a missing row).
  The test enumerates the command list, so a command added later with no
  operator path fails it rather than being noticed in review.
  *Second acceptance:* the same test asserts every one of those successes
  wrote an audit row naming the operator.

### D13 — the whole audit, filterable, and every ruling in it

**Today.** `platform-audit` (`crates/server/src/api/query/platform_audit.rs`),
subscribed live at the head with a row panel showing the typed event
(`crates/app-platform/src/audit.rs:1-11,21`). `Params` already parses
`command`, `subject` and `before` (`crates/server/src/api/query/mod.rs:111-117`).
The `audit` table *is* the change feed and has no retention by ruling
(`docs/rebuild/plan.md` §Product contract), so nothing prunes the record.

**Requirements.**

- **D13.1 — EXISTS.** The whole log, newest first, live.
  *Acceptance (regression):* an e2e that runs a command in one tab and sees the
  row arrive in `/platform/audit` in another.
- **D13.2 — EXTEND. Filters an operator can actually reach.** Actor, hat
  (`acting_as`), object, command and a time range, as controls, with the
  parameters already parsed wired to indexes: `audit_actor_idx (actor_identity_id, id)`,
  `audit_acting_as_idx (acting_as, id)`, `audit_at_idx (at)` and the
  `audit_object` side table from C10.2. A filter with no index is a filter that
  scans the deployment's whole history.
  *Acceptance:* `cargo test -p rn-site --test explain` asserts every filter
  combination the UI can produce is an index seek; the combination test is
  generated from the control's own option lists, so a new filter without an
  index fails the build.
- **D13.3 — DEFER. Tenant-facing audit visibility as a product setting.**
  The mechanism is B5.7 (per-product settings) and the surface is `/org`'s
  audit, which already exists scoped to the organization
  (`Named::OrgAudit`). Deferred with B5.7, and the story's own words carry the
  fence: *tenants don't see this view unless their product turns it on*.

---

## E. Relying parties (OIDC) — leg O1's delivery

### E14 — register, rotate, delete a client; consents go with it

**Today (O1).** `oidc_client` with name, URIs, auth method, `trusted`,
`members_only`, secret hash, created/rotated timestamps
(`crates/kernel/migrations/6_oidc.sql:57-91`); commands `RegisterClient`,
`UpdateClient`, `RotateClientSecret`, `DeleteClient` wired at
`crates/server/src/api/cmd/mod.rs:135-138`, the two secret-minting ones marked
`quiet` so the secret is returned once and never replayed (the `Minted`
contract, `crates/kernel/src/cmd/mod.rs:318-331`).

**Requirements.**

- **E14.1 — EXISTS (O1).** *Acceptance:* O1's own RP proof
  (`crates/server/tests/oidc/rp.rs`).
- **E14.2 — NEW. The operator screen** — this is B6.3. Registration form,
  secret shown exactly once with a stated reason, rotate, delete with the
  consent count it will take with it.
  *Acceptance:* a screenshot of the once-only secret panel, viewed; a test that
  re-requesting the same idempotency key returns no secret
  (`Minted.token == None` on replay).
- **E14.3 — EXTEND. Deleting a client must state what goes.** The count of
  consents and live tokens under it, in the confirm, and in the audit payload.
  *Acceptance:* delete a client with two consents; the audit row's payload
  carries `2`.

### E15 — who consented to which client; bulk revoke

**Today (O1).** Consent is a relation — `oidc_client:X #authorized @person:Y`
(`crates/kernel/src/relation/vocabulary.rs:42-47`,
`crates/kernel/src/relation/kinds.rs:112-123`) with scopes in `oidc_consent`
(`6_oidc.sql:149`); `RevokeConsent` exists.

**Requirements.**

- **E15.1 — NEW.** Query `platform-consents` and the bulk control — part of
  C10.1. Bulk revoke is one command per consent under one idempotency key
  *per consent*, not a new bulk command: a partial failure must leave a record
  of exactly which ones went.
  *Acceptance:* revoke in bulk across three consents with the second one
  already revoked; three audit rows exist and the answer names the one that
  was already gone.
- **E15.2 — EXTEND.** The person page shows the same list for one person
  (C7.1), and `/app` shows a person their own (O1's `authorizations` query).

---

## F. Data lifecycle

### F16 — a consistent backup, restored onto a fresh formation

**Today.** Nothing in the tree. `deploy/CUTOVER.md` is a fresh-formation
runbook and `docs/rebuild/plan.md` §Release says the live state is *archived,
not migrated*. hiqlite owns the state machine
(`crates/server/src/db/cluster.rs`), so a file-level copy of a running voter's
directory is not a consistent backup.

**Requirements.**

- **F16.1 — NEW, and it opens with a verification spike.** hiqlite 0.14's own
  backup/restore surface must be read before anything is designed on top of
  it: whether it exposes a leader-side snapshot, what it writes, and how a
  restore is expressed (a marker file, a config flag, an API). The leg's first
  commit is that finding, in `docs/ops-backup.md`, with the version pinned.
  If hiqlite offers none, the fallback is a **logical** backup — every table
  streamed out in dependency order through `ReadStore` at one feed offset,
  with the offset in the manifest — and that fallback is the design, not a
  disappointment: it is portable across schema versions and is the same walk
  F17 needs.
  *Acceptance:* the spike commit names the API or its absence, with the doc
  link and the version.
- **F16.2 — NEW. `rn-site admin backup <dir>`.** Writes a manifest (offset,
  schema hash, node, timestamp, row counts per table) and the data. It refuses
  unless it can name the offset it is consistent at — a backup that cannot say
  what it contains is not one.
  *Acceptance:* back up an ephemeral instance with a known number of persons;
  the manifest's counts equal `SELECT count(*)` for each table.
- **F16.3 — NEW. `rn-site admin restore <dir>` onto an empty formation.**
  Refuses a non-empty database outright — restore is a formation act, not a
  merge. On completion the health screen (B4.3) reports the restored offset as
  the feed head, which is the story's *proven by the health screen*.
  *Acceptance:* the round trip, as a script: ephemeral instance A seeded with
  a person, an org and a document; backup; `reset`; restore; every public id
  resolves to the same rows (the id key is in the state directory and survives
  a reset only if backed up — so the manifest carries the id-key *fingerprint*
  and the restore refuses a mismatch rather than silently renaming everything).
- **F16.4 — EXTEND. `tools/ephemeral.sh backup|restore` wrapping the two.**
  So the round trip is one command in the loop a developer already uses.
  *Acceptance:* the F16.3 script is that wrapper.

### F17 — export an organization as a portable unit, import one

**Today.** Nothing. The resource registry (`crates/kernel/src/resource/`) is
what makes it possible in principle — everything an organization owns is a
`resource` row with `owner_party_id`.

**Requirements.**

- **F17.1 — DEFER, with the shape written down.** The story itself says *may
  land later*. It is deferred because the walk it needs is F16.1's logical
  backup restricted to one owner subtree, and building the restricted walk
  before the full walk exists means guessing at the boundary twice. What
  unblocks it: F16.2 shipping, and a second product existing (with one
  product, "everything an organization owns" is documents, and the export
  format would be shaped by that accident).
  When it lands: `ExportOrganization` produces a signed archive
  (manifest + rows + a public-id map), `ImportOrganization` mints new parties
  and rewrites ids through the map, never reusing an id from another
  deployment — no federation is a ruling
  (`docs/stories/platform-admin.md` §Rulings).
  *Acceptance when it lands:* export from instance A, import into instance B,
  and every relation inside the subtree resolves in B while nothing outside it
  came across.

### F18 — wipe a disposable-dev deployment and bootstrap it again in one command

**Today.** `tools/cluster.sh stop` removes the whole run directory
(`tools/cluster.sh:165`); `just clean` removes `target/cluster`;
`tools/ephemeral.sh reset` (this leg, `tools/ephemeral.sh:cmd_reset`) wipes
one worktree's state; `tools/seed.sh` bootstraps an operator through the real
form.

**Requirements.**

- **F18.1 — EXISTS for the ephemeral instance.**
  `tools/ephemeral.sh reset && tools/ephemeral.sh up` is the one-command wipe
  and rebootstrap; the dev sign-in invents the operator on first use
  (`crates/server/src/auth/dev.rs:50`), so no seed step is needed.
  *Acceptance:* proven in this leg — reset, up, sign in as operator, land on
  `/platform`.
- **F18.2 — EXTEND. `tools/cluster.sh reset`** — stop, wipe, start, seed, in
  one word, for the three-voter case.
  *Acceptance:* `tools/cluster.sh reset` ends with three ready nodes and an
  operator that can sign in.
- **F18.3 — NEW. `rn-site admin wipe` refuses outside dev.** A subcommand that
  can empty a deployment must read `mode` and decline in `prod`, and the
  decline is loud, not uniform — the audience is an operator at a terminal.
  *Acceptance:* `RN_SITE__MODE=prod rn-site admin wipe` exits non-zero with a
  sentence and touches nothing.

---

## G. Testing and operating safely

### G19 — one-click operator sign-in on a dev build, absent from a release

**Today.** Complete and well-gated. `POST /auth/dev`
(`crates/server/src/auth/dev.rs:84`) under `#[cfg(debug_assertions)]`, refused
unless `RN_SITE__DEV=1` *and* dev mode, answering `404` rather than `403`
(ibid:12-16); the kernel entry is a command-shaped transaction writing
`dev-sign-in` as its command name with a `sign-in` payload
(`crates/kernel/src/dev.rs:62-70`); it invents
`dev-operator@example.invalid` when the deployment has no operator (ibid:50).

**Requirements.**

- **G19.1 — EXISTS.** *Acceptance (regression):*
  `crates/server/tests/dev_sign_in.rs` and the negative-space row asserting
  the route 404s in a release profile.

### G20 — a local three-voter cluster, and an ephemeral instance per worktree

**Today.** `tools/cluster.sh` runs three real voters on 3161-3163 with a shared
peer map and secrets (`tools/cluster.sh:17-38`), `kill <n>` SIGTERMs one
(ibid:131), and `status` reports each node's `/readyz`. The health screen tells
the truth about the node it is served from (`crates/server/src/ops.rs:96`).
`tools/ephemeral.sh` (this leg) gives each worktree its own instance:
`sha256(worktree path) mod 100` → app `3300+slot`, raft `8300+slot`, hiqlite
API `8400+slot`, state under `target/ephemeral/`, its own id key and signing
key, `RN_SITE__DEV=1`.

**Requirements.**

- **G20.1 — EXISTS.** Three voters; kill one and commits continue; kill two and
  writes refuse with the uniform decline.
  *Acceptance (regression):* `tools/perf/drill.sh`, evidenced in
  `docs/perf/2026-08-27-f5/drill-{1,2}.txt`.
- **G20.2 — EXISTS (this leg).** Two instances up at once, no collision, no
  residue.
  *Acceptance:* two directories, `up` in both, both `/readyz` `"status":"ok"`
  on different ports, `stop` both, `pgrep -f rn-site` empty. Run; the
  transcript is in this leg's report.
- **G20.3 — EXTEND. The health screen must tell the truth about the *cluster*,
  not the node.** That is B4.1/B4.2 — the story's "the health screen tells the
  truth" is only half-true today, because a killed node's absence is invisible
  from its peers' screens.
  *Acceptance:* B4.1's.

### G21 — a way in that does not depend on the web tier

**Today.** One subcommand: `rn-site bootstrap-operator <email>`
(`crates/server/src/main.rs:71`). It opens the database directly, and hiqlite
holds an exclusive lock on its data directory — so **it cannot run beside a
live voter**, which `crates/server/src/bootstrap.rs:15-27` states plainly.
There is no node-local route.

**Requirements.** Both halves, because they cover different failures.

- **G21.1 — NEW. `rn-site admin <cmd>` against a stopped node's store.**
  Subcommands: `operators`, `grant-operator <email>`, `revoke-operator <email>`,
  `sessions [--person]`, `revoke-session <public-id>`, `products`,
  `enable <slug>` / `disable <slug>`, `audit --tail <n>`, `backup` / `restore`
  / `wipe` (F16, F18). Each runs the *same kernel command* the API runs, with
  a principal built from `--as <email>` — the CLI is a second caller, never a
  second authorisation path, so every action lands in the audit with an actor.
  It refuses when the data directory is locked, and the refusal names the pid
  holding it.
  *Acceptance:* against a stopped ephemeral instance, `admin grant-operator`
  writes a `relation` row *and* an audit row; run against a running one, it
  exits non-zero naming the lock.
- **G21.2 — NEW. A node-local operator route, off unless configured.**
  `RN_SITE__ADMIN_ADDR` (e.g. `127.0.0.1:3399`) opens a **second listener**
  serving `/admin/*`. No cookie, no session: reaching the socket *is* the
  authority, which is defensible only because the socket is loopback — so the
  config refuses any address that is not loopback, at boot, in both modes. It
  serves what G21.1 serves, over HTTP, on a *running* node — which is the case
  the CLI cannot reach and the one the story is actually about ("when
  something is wrong"). Every action is still a command with an audit row,
  actor `--as`, and the route set is the smallest that unwedges a deployment:
  read the audit tail, list and revoke sessions, list and grant operators,
  toggle a product, read `/readyz` in full.
  *Acceptance:* boot with `RN_SITE__ADMIN_ADDR=0.0.0.0:3399` and the process
  exits at config validation with a sentence; boot on loopback, and
  `curl 127.0.0.1:3399/admin/audit?tail=5` returns rows while
  `curl <public addr>/admin/audit` 404s; a route-matrix row asserts `/admin/*`
  is absent from the public router for all six principals.

---

## Counts

| mark | count |
|---|---|
| EXISTS | 10 |
| EXTEND | 18 |
| NEW | 26 |
| DEFER | 4 |
| **total requirements** | **58** |

Across all 21 stories. Counted by
`grep -oE '^- \*\*[A-G][0-9]+\.[0-9]+ — (EXISTS|EXTEND|NEW|DEFER)'`.

DEFER, with what unblocks each: **A3.3** operator second factor (a WebAuthn
factor kind); **B5.7** per-product settings (a second setting);
**D13.3** tenant audit visibility (B5.7); **F17.1** organization
export/import (F16.2 shipping, and a second product existing).
