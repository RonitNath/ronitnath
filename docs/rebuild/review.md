# rn-site rebuild — adversarial review (leg R1, 2026-08-27)

Read-only first, then fixes for what could be proved. The reviewer did not
write any of the code under review. Base revision `ca77842` (`rebuild` at the
end of P1/F5); review branch `rb-r1`.

Everything below is a command that was run and its output, or a finding with a
repro. Where a row could not be run on this machine it says so and why, rather
than being left blank.

```
machine    Apple M4, 10 cores, 16 GiB, Darwin 25.2.0 arm64
rustc      1.97.1 · trunk 0.21.14 · CARGO_BUILD_JOBS=8
worktree   /Users/ronitnath/dev/worktrees/rn-site--r1
```

## Findings, by severity

| # | severity | file:line | what | repro | fixed |
|---|---|---|---|---|---|
| 1 | **high** | `crates/server/src/sub/mod.rs:236` (pre-fix) | A declined subscription read was treated as an empty result set, so `query::ops` turned every key an event named into a `del` and sent it. The socket checks no tier, so any member could subscribe to `platform-identities` and be handed the public id of every registration the deployment touched. | §Trust boundaries — *a subscription flood* / *another tenant* | **yes** `28c6e7c` |
| 2 | **high** | `crates/kernel/src/cmd/sign_in.rs:26` (pre-fix) | `Disable` moves `party.status`; `SignIn` asked only `identity.status`, a column no command writes. Disabling an account took its open sessions and nothing else — one form post later it had a new one. | §Trust boundaries — *an operator disabling themselves* | **yes** `6a22460` |
| 3 | **high** | build, `crates/server/src/assets.rs:44-58` | `cargo test --workspace` — the gate manifest's whole fast suite — did not compile on a fresh checkout. `#[derive(Embed)]` is a hard error on a missing folder and the four `dist/` folders are trunk output and gitignored. `.forgejo/workflows/deploy.yml` checks out `clean: true` and never runs trunk. | §Gate manifest, row *format / lint* | **yes** `8683df4` |
| 4 | **medium** | `crates/kernel/src/cmd/{revoke_session,disable,enable,add_factor,remove_factor,sign_out}.rs` and `crates/kernel/src/merge/{rule,candidate,confirm,split}.rs` | Ten commands bound `cmd::member`'s middle value — the *person* — into `audit.acting_as`. After `ActAs` that is the wrong party, and attribution is the whole of what acting as an organization means. (The F4 note named six; the four merge commands have it too.) | §Known finding | **yes** `b23c64c` |
| 5 | **medium** | `crates/kernel/src/cmd/{create_organization,claim_link,leave}.rs` | Three commands bound one SQL parameter twice: `$3` is the `audit.acting_as` column *and* was read as the event's own subject. After `ActAs` the change feed said an organization founded, joined and left things its member did — and `org::membership_touch` keys a diff off `Event::Left { party }`, so the socket re-read a row nobody moved. | §Model conformance — *events are command-emitted* | **yes** `2eb527a` |
| 6 | **medium** | `crates/server/src/api/cmd/mod.rs:180` (pre-fix) | `x-rn-after` was a pre-auth tarpit: the read-your-writes barrier runs before the cookie is resolved, and waited the full 2 s for any offset a caller typed. Anonymous `curl` with `sec-fetch-site: same-origin` held a request slot for **2.002 s** before its 403. | §Trust boundaries — *a forged/huge `x-rn-after`* | **yes** `3165f28` |
| 7 | **medium** | `crates/server/src/sub/mod.rs` (pre-fix) | No cap on `SubRequest.subscribe`. Each subscribed query is re-read once per feed event; 16 sockets (`PER_IDENTITY`) × N parameterisations from one account is N×16 reads per commit. `Params::subject` also took a string of any length while its sibling `id` capped at 32. | §Resource/lifecycle matrix | **yes** `3165f28` |
| 8 | **medium** | `crates/server/src/api/query/org.rs:259` (pre-fix) | `org-members`' `PartyDisabled`/`PartyEnabled` arm named the party's key with no container guard, so a deployment-wide disable handed one organization's reader the public id of a party that may be nobody's member. | §Trust boundaries — *another tenant's `?org=`* | **yes** `28c6e7c` |
| 9 | **low** | `crates/kernel/migrations/1_kernel.sql:47`, `:210` | Two status values in the schema have no producing command, which §Model says makes them non-existent: `identity.status = 'disabled'` (only `Disable` writes a status, and it writes `party.status`) and `resource.status = 'deleted'` (read as a filter in six statements, written by none). | `rg "status = 'deleted'" crates/*/src` → only a test fixture | no — see §Not fixed |
| 10 | **low** | `crates/server/src/db/migrations.rs:1-12` | Module docs say "Until K1 hands over `rn_kernel::migrations()`, `Migrations::default` is the empty set and boot applies nothing". K1 landed: `main.rs:29` reads `Migrations::embedded::<rn_kernel::Migrations>()`. Stale prose in a live file. | read the two files | no — see §Not fixed |
| 11 | **low** | `crates/ui/tokens.css:68` | `--radius: 0.25rem` against `interface-taste.md` §Color, shape: "no rounded corners". Applied to four token rules and six call sites. A taste call, so presented rather than changed. | §Visual conformance | no — owner's call |
| 12 | **low** | `crates/ui/Cargo.toml`, `crates/app-member/Cargo.toml`, `crates/app-org/Cargo.toml` | Six unused direct dependencies: `ui: uuid, wasm-bindgen, wasm-bindgen-futures`; `app-member: wasm-bindgen, wasm-bindgen-futures`; `app-org: web-sys`. `clap` sits in `[workspace.dependencies]` and no member names it. | §Dependency hygiene | no — see §Not fixed |
| 13 | **low** | `crates/server/tests/surface.rs:602` | `the_deleted_vocabulary_is_gone_from_the_tree` walks `crates/server/src` only; the plan says "no `manage`/`capability`/`resource_grants` symbol **in the tree**". Verified by hand tree-wide and clean, so this is a gap in the gate rather than a hole in the code. | §Negative space | no — see §Not fixed |
| 14 | **low** | `crates/kernel/src/cmd/sign_in.rs:96` | `acting_as = person_id.unwrap_or(Id::<Person>::new(identity_id.get()))` re-reads an *identity* rowid as a *party* rowid — the same table-confusion `disable.rs:41` calls out as a bug it removed. Unreachable in this cut (`Register` always writes `person_id`), so it is a latent trap rather than a defect. | read `sign_in.rs:94-98` | no — see §Not fixed |
| 15 | **medium** | `crates/server/src/links/mod.rs:120` | The claim page cannot name the one grant this cut mints. `Grant::of` reads only `link.verifies_factor_id`; an invitation carries its grant as a relation row, so it falls to `Grant::Unknown` and the page says "An invitation" — not which organization, who minted it, or at what role. | §Visual conformance V2 | no — owner's call |
| 16 | **low** | `crates/app-platform/src/…` (the audit table) | `/platform/audit` renders **Acting as** as a raw public id while **Actor** beside it renders a display name. | §Visual conformance V3 | no — owner's call |
| 17 | **low** | `templates/landing.html` / `crates/ui/tokens.css` | The wordmark is red on the dark landing, amber on the light landing, white on `/auth`, near-black on the light claim page — and light mode *inverts* the name/company pair. "A wordmark is one color" (2026-06-06). | §Visual conformance V1 | no — owner's call |
| 18 | **low** | `end2end/tests/starscape.spec.ts:217,381` | Two e2e tests read a label off a live animated sky and then act on it; both failed in one full serial run and passed alone and in a second full run. | §Golden flows | no — starscape's owner |
| 19 | **low** | `docs/rebuild/plan.md` §Product contract, §API (pre-fix) | Three plan claims the code contradicts: a "change-feed retention" recurring job that does not exist (the lane runs the last_seen drain, match scanning, and the session and link sweeps); `/api/q` described as "cacheable `private, no-store`", which is a contradiction and the code sends `no-store`; and a command reply vocabulary of `200/409/403/404` that omits the `422` and the `503` the code sends. | read `crates/server/src/observe.rs`, `http::cache_policy`, `api::decline` | **yes** — plan text corrected, not bannered |
| 20 | **medium** | `crates/kernel/migrations/1_kernel.sql:236` (`audit`) | The audit table is the change feed and it has **no retention, no compaction and no bound**. Every command appends a row; every subscription, the invalidator and the observation lane read it forward by `id`. It is correct to keep an audit log forever, and it is not correct for the *feed* to be the same table with no horizon — the kernel report's log group has "retention by cursor horizon" precisely because of this. Unbounded, but bounded-growth-per-command and read by an index, so it degrades in disk rather than in latency. | `rg -n "DELETE FROM audit" crates` → nothing | no — rung 7 owns it |

## Gate manifest — every row, with its command

| invariant | command | result | evidence |
|---|---|---|---|
| format | `cargo fmt --check` | **pass** (exit 0, no diff) | §Acceptance |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` | **pass** | §Acceptance |
| kernel contract | `cargo test -p rn-kernel` | **pass** | §Acceptance |
| no full scan | `cargo test -p rn-kernel --test explain` | **pass** | §Acceptance |
| route/auth matrix | `cargo test -p rn-site --test surface` | **pass** | §Trust boundaries |
| negative space | `cargo test -p rn-site negative_space` | **pass**, with finding 13 | §Negative space |
| GET never writes | `cargo test -p rn-site read_store_has_no_execute` | **pass** | §Negative space |
| bundles build | `trunk build --release` ×4; `cargo check --target wasm32-unknown-unknown …` | **pass** | §Bundles |
| size gates | `tools/size-gate.sh` | **pass**, 16 warnings | §Size gates |
| golden flows | `end2end/` playwright, `--workers=1 --project=chromium` after `tools/seed.sh` | see §Golden flows | §Golden flows |
| visual | agent-browser, `/`, `/auth`, `/links/<t>`, 3 routes per tier, 1440/390, both themes | see §Visual conformance | §Visual conformance |
| performance | `docs/perf/2026-08-27.md`, `docs/perf/2026-08-27-f5.md` | **verified, not re-run** | §Performance |
| resilience | `tools/cluster.sh` kill-one / kill-two / restart | **verified from F5 evidence, not re-run** | §Resilience |
| release build | Containerfile; CI fast suite | **not runnable here** — deploy is out of R1 scope | §Release build |

## Trust boundaries — the attacks, and what happened

`docs/rebuild/plan.md` §Product contract names four surfaces: `public`,
`browser-session`, `recipient/embed` and `service-principal` (none in this
cut). Each attack below was run, either as a request against a live node or as
the test that already asserts it; where the answer was wrong it is a finding
above.

| attack | answer | how it is enforced |
|---|---|---|
| **bearer token on `/api/*`** | `403`, the uniform decline — a link token is nobody there | Structural: `links::Bearer` implements `FromRequestParts<LinkState>` and nothing else, so a bearer route under `/api` is a compile error (`links/mod.rs:13`, a `compile_fail` doctest). Runtime half: `a_link_token_is_nobody_on_the_api_and_a_cookie_opens_no_link` (`surface.rs:479`). |
| **cookie on `/links/<t>`** | The claim page opens on the *token*; the cookie only decides whether the grant has somewhere to land | `links::claim` requires both, and an anonymous claim redirects to `/auth?next=/links/<t>` rather than silently creating anybody (`links/mod.rs:214`). Same test. |
| **forged `p_` id** | Uniform decline, without a query | `ids::decode` checks the prefix, then the AES tag, then the row bound — three refusals with one answer (`ids/mod.rs:52-72`). `refs::` turns every one of them into `decline()`. Proptest: `crates/kernel/tests/properties.rs` id round-trip / forgery rejection. |
| **another tenant's `?org=`** | Uniform decline, indistinguishable from "no such organization" | `org::scope` decodes the id, reads `membership` *on this request*, and requires `admin` or better (`api/query/org.rs:61-88`). It is re-read every request rather than believed from the shell that served the bundle. Covered by `crates/server/tests/org.rs`. **Finding 8** was the one place a tenant boundary leaked, through the subscription rather than the read. |
| **replayed idempotency key, different body** | `409`, uniform body | `audit.key` is `UNIQUE NOT NULL` and `request_digest` tells a replay from a reuse (`kernel/src/audit.rs:76`). The collision is at the database, not at a check a command could skip. |
| **a GET that writes** | Unrepresentable | A query handler is handed `ReadStore`, which has no `execute` and no `commit`; two `compile_fail` doctests in the kernel and `read_store_has_no_execute` (`surface.rs:625`) scanning `src/api/query` for `.execute(`, `.commit(`, `INSERT `, `UPDATE `, `DELETE `. |
| **`Sec-Fetch-Site: cross-site` on a command** | `403` before the body is parsed | `origin::SameOrigin` is an extractor and sits ahead of the body in the handler signature, so a cross-site post is refused rather than answered with a `415` about its content type (`api/origin.rs:47`). `same-site` and an *absent* header fail too. `nothing_mutates_without_the_browser_saying_where_it_came_from` (`surface.rs:516`). |
| **oversized body** | `413`, measured: a 3 MB `create-document` body answered `413` | axum's 2 MiB `DefaultBodyLimit`. Not an rn-site control — inherited, and classified `Accept` here because the surface takes only JSON envelopes of a few kilobytes and the command deadline (5 s) bounds the slow-body case on top of it. |
| **a subscription flood** | `1013` close, per-identity and per-node | `sub::limits::Connections` — 16 per identity, 4 096 per node, counted in one shared registry with an RAII guard so a panicking task returns its slot (`sub/limits.rs`). What it did *not* bound was queries per socket — **finding 7**. |
| **a 10⁴-group principal** | Bounded by what a human joins, and the query shape does not degrade | `check()` is one indexed query over the expanded subject set: `json_each($1) CROSS JOIN relation ON r.subject_key = s.value`, one seek per subject, loop order pinned so a widely-shared object cannot make it one pass per grant (`relation/check.rs:44`). `crates/kernel/benches/check.rs` runs the adversarial shapes. `expand` itself is one indexed read of `membership`. |
| **a link claimed twice** | Uniform decline, second time | `link.claimed_by_identity_id IS NULL AND expires_at > $4` is inside the batch's guarded statement, so the second claimant loses the race in the transaction rather than after it (`cmd/claim_link.rs:29`). Unknown, expired and claimed are one answer. |
| **a merged party's session** | Still works, and that is the model | Sessions bind `identity_id`, never `person_id` (§Schema), so a merge leaves every session live. `principal::resolve` reads the identity's current `person_id`, so the next request speaks as the survivor. `merge_properties.rs` asserts product tables reference identity. |
| **an operator disabling themselves** | Permitted, and it takes their sessions | `disable::authorised` allows a person to disable their own party. **Finding 2** was that it did not stop them signing back in; it does now, and `Enable` is the way back. |
| **the last owner leaving** | Uniform decline | `cmd/leave.rs:38` — `role == Owner && owner_count <= 1` declines. The way out is `Transfer`, which is a decision about a successor rather than a decision to stop being here. |
| **a forged / huge `x-rn-after`** | Skipped, and answered | **Finding 6**. The barrier now runs only for a request carrying a cookie, and an offset more than `MAX_LEAD` (10 000) ahead of this node's head is not waited for. |
| **a command that never completes** | `503` with the uniform decline body | `http::deadline` — 5 s for anything that is not a GET, 2 s for `/api/*` reads, none for `/api/sub` or for documents and assets. `503` and not `408`: what timed out is the cluster's ability to commit, not the caller. A timed-out command is *undecided*, which is what the idempotency key is for. |

### The same attacks, against a live node

The table above cites the tests. These are the requests, run by hand against
the seeded dev node on `127.0.0.1:3004` — because a test that mocks the
transport is not the transport.

```
bearer token in the session cookie on /api/q/sessions        403
POST /api/cmd/sign-out, sec-fetch-site: cross-site           403
POST /api/cmd/sign-out, no sec-fetch-site at all             403
POST /api/cmd/disable, party=p_AAAAAAAAAAAAAAAAAAAAAA        403 {"decline":"declined"}
GET  /api/q/org-members?org=o_AAAAAAAAAAAAAAAAAAAAAA         403 {"decline":"declined"}
GET  /api/q/nonesuch                                         404
GET  /api/q/../../etc/passwd                                 404
GET  /api/cmd/create-document        (a write over GET)      405
POST /api/cmd/create-document, 3 MB body                     413
POST same key, same body                                     200, the original reply
POST same key, different body                                409 {"decline":"declined"}
POST /api/cmd/leave, the org's only owner                    403 {"decline":"declined"}

as a plain member holding no operator relation:
  /api/q/platform-parties · -identities · -audit · -sessions  403 ×4
  GET /platform                                               404
  GET /org      (belongs to no organization)                  404
  GET /app                                                    200
anonymous:
  GET /platform                                               404
  GET /app                                                    302 → /auth?next=/app

a link, end to end:
  GET  /links/<token>                                          200
  POST /links/<token>/claim   (signed in as somebody else)     303 → /app
  GET  /links/<token>          (the same token, second time)   404
  POST /links/<token>/claim    (again)                         404
  GET  /links/AAAA…            (a forged token)                404

disable, end to end — the shape of finding 2:
  POST /api/cmd/disable on my own person                       200
  GET  /api/whoami on the session I ran it from                403
  POST /auth/sign-in with the same password                    403   ← was 303
```

`GET /api/whoami` on the operator's session, in full, is the whoami contract
holding:

```json
{"identity":{"public_id":"i_y5eVLLhrhnVQnnpKkVdFrA","display":"Operator"},
 "person":{"public_id":"p_DZhS735xBTdLxwnEXyM9jA","display":"Operator"},
 "acting_as":{"kind":"person","public_id":"p_DZhS735xBTdLxwnEXyM9jA","display":"Operator"},
 "tiers":["member","platform"],"organizations":[],
 "session_expires_at":1789051117}
```

No email, no internal id, no relation the chrome does not draw. Every
identifier is a derived public id.

### The default-deny matrix

`crates/server/tests/surface.rs` holds it as data: `CASES` is one row per
`.route(…)` literal in `crates/server/src`, and `every_route_has_a_matrix_row`
scans the source and fails if a route was added without one. Six principals per
row — anonymous, member, org operator, platform operator,
authenticated-but-unauthorized, bearer. Three shell answers are asserted
separately and the asymmetry is deliberate: `/platform` 404s the anonymous
visitor as well as the unauthorized one, because a redirect to sign in would
confirm the route exists.

`403` and `404` return **identical bytes** (`{"decline":"declined"}`); only the
status differs, chosen so a browser behaves. Asserted at
`api/decline.rs:126`.

## Model conformance

### Schema, table by table, against the kernel report §Schema

Every difference below is either the plan's own stated rule or is carried with
a written reason in the migration. Nothing is unaccounted for.

| table | report | schema | difference, and its reason |
|---|---|---|---|
| `party` | id, public_id, kind, display_name, status, created_at | id, kind, display_name, status, created_at | **no `public_id` column** — §Model: it is *derived* (AES-128 of `table_tag ‖ rowid`), and a ciphertext column would be a second source of truth. Holds for every table below; not repeated. |
| `identity` | id, public_id, source, person_id?, home_zone, status, created_at | same, minus public_id | — |
| `factor` | id, identity_id, kind, value/secret, verified_at | + `created_at` | additive |
| `person_link` | id, identity_id, person_id, method, evidence, asserted_by, at | + `from_person_id` (migration 3) | stated: Split reads it to restore the set an identity arrived with, so the inverse is a real inverse rather than a detach |
| `person_alias` | old_person_id → person_id | + `at` | additive |
| `match_candidate` | identity_a, identity_b, signal, score, status | + id, created_at, `CHECK (identity_a < identity_b)` | the CHECK is what makes a pair one row per signal however it was observed |
| `membership` | group_id, party_id, role, at, PK(group_id, party_id) | exact | — |
| `session` | id, identity_id, acting_as, token_hash UNIQUE, expires_at, created_at | + `last_seen_at` | stated: an observation, written on the async lane at most once per 5 min; nothing authoritative reads it |
| `link` | id, token_hash UNIQUE, expires_at, claimed_by_identity_id? | + claimed_at, verifies_factor_id, created_at | `verifies_factor_id` is stated: there is no relation that expresses "this token verifies that factor" |
| `resource` | id, kind, public_id, owner_party_id, home_zone, status, created_at | same, minus public_id | — |
| `relation` | object_kind, object_id, relation, subject_kind, subject_id, granted_by, at; UNIQUE(object, relation, subject); idx(object); idx(subject) | all of it, + `id`, + `subject_key` GENERATED VIRTUAL, + `relation_subject_key_idx`, + `relation_granted_by_idx` (migration 4) | stated: one text subject so `check()` seeks a *set* of subjects through one index instead of one `OR` branch per kind. Generated, so it cannot disagree. |
| `audit` | id, command, actor_identity_id, acting_as, at, payload, event_offset | id, key UNIQUE, command, actor_identity_id, acting_as, at, request_digest, payload | **no `event_offset`** — stated: `audit.id` *is* the offset, and a second counter could disagree with the row order it describes. `key`/`request_digest` are the idempotency contract §API asks for. |

Kind tables (`party_resource`, `document`) hang off `resource` 1:1 as §People
requires; `party_resource.party_id` is `UNIQUE`, so a party is owned through
exactly one resource row.

### The five invariants

| claim | verdict |
|---|---|
| **product tables reference identity, never person** | Holds. `factor.identity_id`, `session.identity_id`, `link.claimed_by_identity_id`, `relation.granted_by`, `audit.actor_identity_id`, `match_candidate.identity_{a,b}` all bind identity. `document` hangs off `resource`, which names an *owner party* — which is the ownership axis, not the product one. `merge_properties.rs` asserts it as a property. |
| **groups never own** | Holds, structurally. `1_kernel.sql` has two triggers (`resource_owner_is_never_a_group_{insert,update}`) that `RAISE(ABORT)` when `owner_party_id` names a `group` or a `service`, and `refs::owner` refuses the same ids before the database is reached — "a caller deserves the same answer whichever layer notices". Proptest in `crates/kernel/tests/properties.rs`. |
| **transfer is the only command that changes an owner** | Holds with the stated exception. `Transfer` is the only *command*; a merge moves `resource.owner_party_id` and the `#owner` row onto the survivor and `Split` moves them back, "because there the owner is not changing, the person is" (plan §Model, and `merge/apply.rs`). |
| **`check()` is one indexed query** | Holds. `CHECK_SQL` is one statement, two `UNION ALL` branches, both driven from the subject side with `CROSS JOIN` pinning the loop order. `crates/kernel/tests/explain.rs` asserts the index for each. The `CROSS JOIN` note is the good kind of comment: without the pin the planner drives from the object, which makes a check cost one pass per grant — "a number a stranger chooses by sharing something widely". |
| **events are command-emitted; audit inside the transaction** | Holds. `kernel::feed` has **no `append`**, deliberately: the event is the command's own audit statement, inside the command's own batch, and every statement in the batch carries the same guard. `feed::notify` runs after the commit, never inside it, and is explicitly best-effort. **Finding 5** was the one place the emitted event named the wrong row. |

### Status vocabularies — does every value have a producing command?

| column | value | produced by |
|---|---|---|
| `party.status` | `active` | `Register`, `CreateOrganization`, `CreateGroup`, `Enable` |
| | `disabled` | `Disable` |
| | `merged` | `ConfirmMatch` / `RuleMatch`, via `merge::apply` |
| `identity.status` | `active` | `Register` |
| | `disabled` | **nothing** — finding 9 |
| `session` | (no status; the row is the session) | — |
| `resource.status` | `draft` | `CreateDocument` |
| | `published` | `PublishDocument`, `CreateOrganization`, `CreateGroup` |
| | `deleted` | **nothing** — finding 9; six statements filter `<> 'deleted'` and none writes it |
| `match_candidate.status` | `proposed` | `ProposeMatch`, and the observation lane's scan |
| | `confirmed` | `ConfirmMatch`, `RuleMatch(same_person)` |
| | `rejected` | `RuleMatch(!same_person)` |

Both gaps are the *same* shape as the ones the model doc rules out, and both
are defensible as schema-ahead-of-code the way `factor.kind IN
('passkey','oidc')` is — except that those two carry a written reason in the
migration and these do not. See §Not fixed.

## Negative space

`cargo test -p rn-site negative_space` — **pass**. Every deleted route answers
`404` to both an anonymous visitor and a signed-in member (asserted for both,
because "a route nobody serves is not a permission"):

```
/manage  /manage/people  /manage/anything/at/all  /api/realtime
/pkg/rn-site.js  /dev-dashboard  /metrics
```

`the_deleted_vocabulary_is_gone_from_the_tree` strips comments first — the
deleted model is discussed in prose all over the tree, and it is the *symbols*
that must be gone — then fails on `capability`, `resource_grants`, `manage`,
`dev_bypass`, `realtime`. It walks `crates/server/src` only (**finding 13**),
so the tree-wide claim was checked by hand:

```
$ rg -w 'capability|resource_grants|manage|dev_bypass|realtime' crates --glob '*.rs'
```

Every hit is a comment, a test's own literal, or `surface.rs`'s word list.
Zero code symbols in any of the eight crates. The gate is narrower than the
plan's sentence; the code satisfies the plan's sentence.

`read_store_has_no_execute` — **pass**. Two halves: the kernel's
`compile_fail` doctests prove `ReadStore` exposes no `execute`/`commit`, and
this scans `src/api/query` for `.execute(`, `.commit(`, `INSERT `, `UPDATE `,
`DELETE ` and asserts a handler is *given* one. That is the right shape — the
type is the control and the scan is the negative proof, rather than the scan
being the control.

## Size gates

`tools/size-gate.sh` — **pass**, exit 0, 16 warnings, 0 failures.

Composition roots (`main.rs`, every `lib.rs`) are all under 200. Sixteen files
sit between 350 and 600 and are warnings by design; the largest are
`api/query/named.rs` (559), `api/query/platform.rs` (488), `starscape/globe.rs`
(444), `ui/table/mod.rs` (426), `sub/mod.rs` (425). Each is one
responsibility — the query vocabulary, the platform vocabulary, one renderer,
one component, one socket — so none of them is a split waiting to happen.

One file **failed** the gate during this review, and it was mine:
`cmd/tests/authority.rs` reached 655 with the two new tests appended. They tell
a different story from the three already there — what a command records, not
who may run it — so they moved to `cmd/tests/attribution.rs` (`ee804e5`)
rather than the limit moving.

## `unsafe`, `unwrap`, `todo!`, `#[allow]`

| inventory | count | detail |
|---|---|---|
| `unsafe` | **12**, 0 unjustified | 4 in `starscape` (`globe.rs:110,120,234`, `render.rs:91`) — WebGL `Float32Array::view` over a Rust slice, the standard wasm-bindgen zero-copy upload, sound as long as no allocation happens while the view lives. 8 in `#[cfg(test)]` blocks (`config.rs`, `tests/cluster_env.rs`) — `std::env::set_var`, which is `unsafe` in edition 2024. |
| `.unwrap()` in production code | **0** | Every hit is inside a `#[cfg(test)]` module. |
| `.expect(` in production code | 138 | Read a sample: they are invariants the type system cannot state (`"a member has a session"` behind a `Session` extractor that only yields members) rather than error handling. |
| `panic!` in production code | **0** | |
| `todo!` / `unimplemented!` | **0** | The single textual hit is inside the `compile_fail` doctest in `links/mod.rs:13`. |
| `#[allow]` / `#[expect]` | **1** | `crates/server/tests/harness/mod.rs:11` — `#![allow(dead_code)]`, on a shared test harness compiled into six test binaries that each use a different subset. Justified. |

For a 52 000-line workspace with a clippy gate at `-D warnings`, that is a
clean sheet.

## Dependency hygiene

`cargo tree -d` reports **42 duplicated crate names** (84 rows). Every one of
them is transitive, and the great majority come in through `hiqlite 0.14` and
its `cryptr`/`s3-simple`/`reqwest` chain: `aws-lc-sys` 0.39 vs 0.44,
`base64` 0.21/0.22/0.23, `rand` 0.8/0.9/0.10, `sha2` 0.10/0.11, `thiserror`
1/2, `tungstenite` 0.29/0.30, `tower-http` 0.6/0.7. The workspace pins one
version of each of these in `[workspace.dependencies]` and every member
inherits it, so no *first-party* crate is on two versions of anything. Nothing
to fix here until the hiqlite fork (plan §Non-goals, rung 8) collapses the
chain.

Unused direct dependencies — **finding 12**:

```
crates/ui           uuid, wasm-bindgen, wasm-bindgen-futures
crates/app-member   wasm-bindgen, wasm-bindgen-futures
crates/app-org      web-sys
[workspace]         clap   (declared; no member names it)
```

Checked by grepping each crate's `src`/`tests`/`benches`/`examples` for the
underscored crate name, then re-checking the hits by hand.

## Resource/lifecycle matrix

`procedures/engineering.md` §Validation asks for one row per externally
triggered or persistent subsystem. Five exist in this cut.

| subsystem | unit | admission | bound behaviour | churn / cleanup | expiry & revocation | telemetry |
|---|---|---|---|---|---|---|
| **subscriptions** (`sub::limits`) | one WebSocket | 16 per identity, 4 096 per node, counted in one shared registry | over either → socket accepted then closed `1013` (*try again later*), because a browser cannot read a status off a failed upgrade | RAII `ConnectionGuard`, so a panicking task returns its slot; `releasing_forgets_the_identity_rather_than_keeping_a_zero` | socket dies with the tab | `tracing::info!(?refused)`, two bounded variants |
| **subscribed queries** (`sub::PER_SOCKET`) | one `(name, params)` | 32 per socket — **added by this review**, finding 7 | the request is truncated at the cap and logged | replaced wholesale on each `hello` | — | one `info` with the cap |
| **outbox** (`sub::outbox`) | one queued message | `CAPACITY` 256 per connection | over → `Resync` and the backlog is dropped, which does not evict valid state, it re-seeds it | drained every loop turn | — | — |
| **observations** (`kernel::observe`) | one session's `last_seen` | `QUEUE_LIMIT` 8 192 distinct sessions, coalesced | over → **drops**, which is the correct backpressure for a value whose loss costs a stale timestamp | coalesced by session, drained on a timer | `THROTTLE` 300 s: at most one write per session per five minutes | — |
| **expiry sweeps** (`sweep_sessions`, `sweep_links`) | one expired row | bounded per tick | takes only what has run out; `the_sweeps_are_bounded_and_take_only_what_has_run_out` | runs on the lane's timer | this *is* the expiry path; revocation is separate and immediate (the row is deleted, and `sub::invalidate` evicts the warm principal) | count per sweep |

Two things the matrix asks for that are missing, both findings above:

* **the audit/feed table has no horizon** (finding 20). Every other persistent
  thing here has a bound; this one does not.
* **the `after` barrier had no admission at all** (finding 6) — an unbounded
  wait, taken before authentication, on a value the caller chooses. Now
  bounded twice: by a cookie being present, and by `MAX_LEAD`.

Restart recovery was exercised by the drill (§Resilience): the observation
lane resumes from its own durable cursor rather than from having been
listening, and the invalidator starts at offset 0 on a cold node, which evicts
nothing and costs one indexed range scan.

## Golden flows

Fresh dev deployment, seeded through the real forms:

```
$ rm -rf data && tools/seed.sh
seed: registered operator@example.invalid
rn-site: operator@example.invalid is now a platform operator (party#1)
$ ./target/debug/rn-site &            # 127.0.0.1:3004, /readyz 200
$ cd end2end && npx playwright test --workers=1 --project=chromium
```

| run | result |
|---|---|
| first | **27 passed, 2 failed** — `starscape.spec.ts:217` (star labels non-empty) and `:381` (a label opens the atlas on *that* star: expected `Sirius`, got `Canopus`) |
| the two, alone | **2 passed** (4.7 s) |
| second full run | **29 passed** (54.2 s) |

So the suite is green and **two starscape tests are flaky** — they read a
label off a live, animated sky and then act on it, and the sky moves between
the read and the act. The first run followed an `npm ci` on the same machine,
which is the load that made it show. Not a regression (R1 touched no starscape
code) and not fixed here, because the fix belongs to whoever owns
`crates/starscape`: pause the clock for the assertion, or assert against the
label the atlas actually opened rather than the one that was read first.

The three journeys the plan names are covered by `member.spec.ts`,
`org.spec.ts` and `platform.spec.ts` and all passed in both runs.

## Visual conformance

52 screenshots under
`/private/tmp/claude-502/-Users-ronitnath-dev/1a80dd12-969c-454a-80ea-f8f4533b4fa6/scratchpad/r1shots/`,
named `<page>-<width>-<theme>.png`: `/`, `/auth`, `/links/<token>` and three
routes per tier (`/app`, `/app/documents`, `/app/groups`; `/org`,
`/org/members`, `/org/documents`; `/platform`, `/platform/audit`,
`/platform/resources`), each at 1440×900 and 390×844 in both themes. Driven
with agent-browser against the seeded node; `/auth` and the claim page were
shot from a second, signed-out session, because signed in they redirect.

**What is right.** Five type sizes and h1 at 1.25× body — the enforced ceiling
is 1.625×, and only the auth page's wordmark reaches 1.6×. No eyebrows, no
kickers, no explanatory quips: the auth page is two peer forms with nothing
but labels, and the claim page is a line and a button. No status dots
anywhere; a state is a word in its own colour (`draft` in amber,
`active` in the body colour), never a tinted capsule. No `box-shadow` in the
whole tree. Counts fold into the title (`Members 1`, `Audit 8`), which is the
2026-06-24 ruling. The rail's footer is pinned to the bottom with the theme
toggle and sign-out on it while the nav list scrolls above — the 2026-08 rail
ruling, shipped. At 390 the rail becomes a `Sections` drawer and the tables
elide columns by priority (`/app/documents` drops Owner and Updated and keeps
Title and State), which is "mobile is information priority" rather than a
breakpoint hack. Every id a human might have to transfer has a copy button.

**Violations, in the order they cost the most.**

| # | where | what | screenshot |
|---|---|---|---|
| V1 | `/` vs `/auth` vs `/links/<t>` | The wordmark is **three different colours**. Red on the dark landing, **amber on the light landing**, white on `/auth`, near-black on the light claim page. Worse, light-mode landing *inverts* the pair: the name is amber and "Isoastra" is red, where dark mode has the name red and "Isoastra" amber. The 2026-06-06 ruling is one colour for a wordmark. | `landing-1440-dark.png`, `landing-390-light.png`, `auth-1440-dark.png`, `claimanon-1440-light.png` |
| V2 | `/links/<token>` | The claim page cannot name what it grants. An actual invitation renders as `Grant::Unknown` → the words "An invitation", because `Grant::of` reads only `link.verifies_factor_id` and an invitation carries its grant as a relation row (`links/mod.rs:120`). It does not say which organization or group, who minted it, or at what role — all of which are one join away. The one grant this cut actually mints is the one the page cannot describe. | `claimanon-1440-light.png` |
| V3 | `/platform/audit` | The **Acting as** column renders a raw public id (`p_DZhS735xBTdLxwnEXyM9jA`) while the **Actor** column beside it renders a display name. An operator console showing an id where a name is available, in the same row, is the interface failing to state. | `platform-audit-1440-light.png` |
| V4 | every tier page | A native `<select>` for **Acting as** in the rail, and another for **Role** in the invite form, beside entirely custom controls everywhere else. "Never ship a native control beside your own control" (2026-02-27). | `app-home-1440-light.png`, `app-groups-1440-dark.png` |
| V5 | `/org`, `/platform` drill-ins, `/links/<t>` at 1440 | Content occupies the top ~300 px and the left ~40 % of a 1440×900 frame and the rest is empty. `/org` is five label/value pairs, three counts and two links in a 1 200 px-wide column. "Empty space is a layout bug, not breathing room" — either the blocks go newspaper-order across the width, or the frame stops being that wide. | `org-home-1440-dark.png`, `claimanon-1440-light.png` |
| V6 | `/` light mode | A white band across the top ~70 px where the page background shows above the sky canvas, with the theme toggle floating on it. Invisible in dark because the band and the sky are both black. | `landing-390-light.png` |
| V7 | tokens | `--radius: 0.25rem` against "no rounded corners" (2026-04-03). Visible on inputs, buttons, the focus ring and the rail's active row. Small enough to be deliberate, so it is presented rather than changed. | `auth-1440-dark.png` |
| V8 | `/app/documents` | Two headings reading `Documents` on one page — the h1 and the table's h2. | `app-documents-390-dark.png` |

None of these is a layout *break*: nothing overflows, nothing scrolls
unintentionally, and both themes render completely at both widths. V1, V2 and
V3 are the three worth fixing before the owner drives it.

## Performance

Not re-run. `docs/perf/2026-08-27.md` (P1) and `docs/perf/2026-08-27-f5.md`
(F5) were read against their own raw evidence, and the numbers in the reports
are the numbers in the files:

| claim | file | line |
|---|---|---|
| commit p99 4.912 ms, p50 2.343 ms, 3 327.5 rps, 8×403 | `2026-08-27-f5/commit-alone.txt` | `n=99834 p50 2.343 ms p99 4.912 ms max 192.754 ms 3327.5 rps errors 8 [200×99826 403×8]` |
| mixed profile 1.425/4.454 ms, 195.4 rps writes; 9.662/16.178 ms, 781.4 rps reads | `2026-08-27-f5/commit.txt` | verbatim |
| fan-out p50 34.712 ms, p99 35.594 ms at 100 subscribers; the commit's own reply 1.440 ms | `2026-08-27-f5/fanout.txt` | verbatim, `0 socket(s) never delivered` |
| documents endpoint p50 9.1 ms, p99 12.5 ms, 876 rps at 8 | `2026-08-27-f5/documents.json` | `p50=0.0091 p99=0.0125 rps=876.0` |
| whoami p50 0.4 ms, p99 0.9 ms, 17 573 rps | `2026-08-27-f5/whoami.json` | `p50=0.0004 p99=0.0009 rps=17573.4` |

The two rows still **OVER** are documented and their mechanism holds:

* **command commit ×4.7** (4.912 ms p99 against ≈1.05 ms). The phase
  decomposition (`phases.txt`) attributes 2.503 ms p99 to the raft transaction
  itself, which is the floor the budget is measured against, and F5's own
  `notify` change took the largest term this leg owned from 0.780 ms p50 to
  0.001 ms. The remaining gap is raft, and the budget's "one intra-zone RTT"
  is being paid on loopback where an RTT is 0.06 ms — so the budget as written
  is nearly all fsync on a laptop. **Verified, not re-litigated.**
* **notification delivery ×34** (35.6 ms p99 at 100 subscribers, 2.7 ms at 1).
  It scales with subscriber count, which is the hiqlite local-read ceiling F5
  names: each woken socket re-reads its own queries locally, and 100 of them
  serialise. The commit's own reply is 1.440 ms, so the *writer* is not
  waiting on the fan-out. **Verified, not re-litigated.**

Both are honest OVERs with a named cause, and both are on the rung-6 (Zenoh)
and rung-8 (multi-raft) side of the plan's non-goals.

## Resilience

Not re-run — the drill needs a quiet machine and three voters, and this one
was serving the visual pass. Read from `2026-08-27-f5/drill-1.txt` and
`drill-2.txt`, two rounds each:

* **kill one (the leader)** — `t+25.0s kill node 1`. The per-second timeline
  dips once (`t+29 committed 344, slowest 247.8 ms` — one election) and
  otherwise holds ~450 commits/s with `refused 0`. **Commits continue.**
* **kill two** — `t+80.2s kill node 3`. Writes stop; the survivor's reads keep
  answering, `20/20 200 in 1.9–4.0 ms`, which is the model's claim that reads
  are local. The refusals are `503` and the slowest is **5 002.6 ms**, which
  is `http::COMMAND_DEADLINE` firing rather than a socket going quiet — the
  behaviour F5 added and the plan's resilience row asks for.
* **restart** — three voters `ok` **4.2 s** after the restart was issued, 0
  retries, and a command commits 2.1 s later. **Converges.**

## Release build

Not runnable here and out of R1's scope (`RAILS: production/deploy out of
scope`). Two observations from reading `Containerfile` and
`.forgejo/workflows/deploy.yml` rather than running them:

* The workflow's gate step runs `trunk --version` — asserting the devshell
  carries it — and then `cargo test --workspace --locked` **without building a
  bundle**. That is finding 3, and `crates/server/build.rs` makes the step
  pass; adding `trunk build --release` to the gates as well would make the CI
  step prove what the release image does, and that edit belongs to whoever
  owns `.forgejo/`.
* The release image builds every bundle before the binary, so the embedded
  sets are the real ones there. The build script changes nothing about that.

## Acceptance

```
$ cargo fmt --check                                        # exit 0, no diff
$ cargo clippy --workspace --all-targets -- -D warnings     # exit 0
$ cargo test --workspace                                    # exit 0
     588 passed, 0 failed, across 33 test binaries
$ tools/size-gate.sh                                        # exit 0
     size gate: clean (16 warning(s))
$ cargo check --target wasm32-unknown-unknown \
      -p rn-api -p rn-ui -p rn-app -p rn-org -p rn-platform -p rn-starscape
                                                            # exit 0
$ (cd crates/<bundle> && trunk build --release)   ×4         # ✅ success ×4
$ pgrep -f rn-site                                          # empty after cleanup
```

## Not fixed, and why

Each of these is a finding with a repro rather than a fix, because fixing it
is a decision this leg does not own.

* **9 — two statuses no command produces.** `identity.status = 'disabled'` and
  `resource.status = 'deleted'`. Both are plausible as schema-ahead-of-code
  the way `factor.kind IN ('passkey','oidc')` is — and *that* one carries its
  reason in the migration, which is what makes it legible. The fix is either a
  migration comment saying which rung writes them, or the values coming out.
  Either is an owner call about the ladder, and migrations 1–3 are frozen, so
  removing a `CHECK` value is a new migration and not an edit.
* **10 — stale module docs in `db/migrations.rs`.** A one-line edit, but the
  file is the server's and correcting prose in somebody else's module while
  claiming to be read-only is how a review starts rewriting. Reported so its
  owner can strike the sentence rather than banner it.
* **12 — six unused direct dependencies.** `wasm-bindgen` and
  `wasm-bindgen-futures` are the ones to be careful with: they are not named
  in the source of the crates that declare them, but the *version* of
  `wasm-bindgen` in the graph has to match the `wasm-bindgen-cli` trunk runs,
  and a pin that only exists transitively is a pin nobody controls. Removing
  them is a five-minute change and a real risk of a bundle that builds today
  and not next week, so it is a finding rather than a fix.
* **13 — the deleted-vocabulary gate walks one crate.** Widening it to the
  workspace is `crates/server/tests/surface.rs`'s to do, and the tree-wide
  check is already clean (§Negative space), so the code is right and the gate
  is narrow. Left as a note rather than a change, because widening a scan
  across seven crates without owning them invites a red gate the next leg has
  to interpret.
* **14 — the identity-rowid-as-party-rowid fallback in `sign_in`.** Dead in
  this cut. Making it unrepresentable means either `identity.person_id NOT
  NULL` (a migration) or `sign_in` declining an unresolved identity (a
  behaviour change to a path nothing reaches). Both are decisions about a
  feature — imported identities — that has not landed.
* **The two flaky starscape e2e tests.** §Golden flows. They belong to
  `crates/starscape`'s owner, and the fix is about how the assertion reads a
  moving sky rather than about the sky.
* **V1–V8 visual.** Presented, not silently picked: taste calls go to the
  owner as variants (`interface-taste.md` §What done looks like).
