# Platform admin — implementation legs (2026-08-27)

The NEW and EXTEND requirements of `docs/stories/platform-admin-requirements.md`,
decomposed into five implementation legs and one validation leg. Each leg is
one worker in its own worktree; the orchestrator merges into `rebuild`.

| leg | owns | depends on | wave |
|---|---|---|---|
| **P1** the kernel's operator | `crates/kernel/**` except `src/product/**`, `src/cmd/product.rs` and migrations 8+; migration `7_authority.sql`; `crates/api/src/commands/platform.rs` | O1 landed on `rebuild` | 1 |
| **P2** products at runtime | `crates/kernel/src/product/**`, `crates/kernel/src/cmd/product.rs`, migration `8_product.sql`, `crates/server/src/product.rs`, `crates/server/src/lib.rs`, `crates/server/src/api/query/platform_products.rs`, `crates/api/src/commands/product.rs`, `crates/app-platform/src/products.rs` | P1 | 2 |
| **P3** the platform surface | `crates/server/src/api/query/platform_*.rs` (except products), `crates/server/src/api/cluster.rs`, `crates/server/src/observe.rs`, migration `9_node_report.sql`, `crates/api/src/cluster.rs`, `crates/app-platform/src/**` (except `products.rs`) | P1 | 2 |
| **P5** the way in without the web tier | `crates/server/src/admin/**`, `crates/server/src/main.rs`, `crates/server/src/config.rs`, `tools/cluster.sh`, `tools/ephemeral.sh`, `docs/ops-backup.md`, `README.md` | P1 | 2 |
| **P4** what the person sees | `crates/server/src/api/whoami/**`, `crates/server/src/api/query/member_*.rs`, `crates/ui/src/**`, `crates/app-member/src/**`, `end2end/tests/impersonation.spec.ts` | P1, P2, P3 | 3 |
| **V1** validation (Sonnet) | nothing — read-only, plus `docs/review/platform-admin/**` | all | 4 |

Three legs run at once, in wave 2, and their owned paths do not intersect.

## The shared registries

Six files are edited by more than one leg, because they are the lists that make
a command or a query exist at all. They are **append-only**: add your line,
alphabetically inside the block it belongs to, and never reformat, reorder or
re-wrap a neighbour's line. A conflict in one of these is one line against one
line and the orchestrator resolves it in seconds; a conflict caused by
reformatting is not.

```
crates/api/src/commands/mod.rs           module decls + ALL_COMMAND_NAMES
crates/server/src/api/cmd/mod.rs         the bindings! table
crates/server/src/api/query/named.rs     Named + Scope + touched()
crates/server/src/api/query/mod.rs       Params fields
crates/server/src/api/query/platform.rs  the Platform enum
crates/kernel/src/event/mod.rs           the Event kinds (+ payload.rs)
docs/rebuild/plan.md                     §Model, §API, §Gate manifest
```

`crates/kernel/src/cmd/mod.rs` is P1's in wave 1 and append-only after it
merges: P2's product command adds one `mod` line and one `pub use` line to the
already-merged file. Migration files are never shared — P1 writes 7, P2 writes
8, P3 writes 9, and nobody edits an applied one.

---

## P1 — the kernel's operator

```
WORKSPACE: ~/dev/worktrees/rn-site--p1 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-p1`, cut from `rebuild` after O1 has landed). Do not touch other worktrees.
  Seed target/ from the main checkout (cargo clean there first) — a cold debug build is
  the slowest thing in this leg.

READ FIRST, in order: docs/stories/platform-admin-requirements.md (§Findings, A2, A3,
  C9, C10.2, C11, D12 — your whole scope); docs/design/platform-admin/dist/index.html
  served locally, and the board's frames B1, B2, C, D; docs/rebuild/plan.md (§Model,
  §API — you extend both); docs/kernel/index.html; crates/kernel/src/cmd/mod.rs (the
  command shape, `is_platform_operator`, `member`), cmd/refs.rs, cmd/disable.rs,
  cmd/enable.rs, cmd/set_role.rs, cmd/revoke_session.rs, cmd/act_as.rs, cmd/sign_in.rs,
  merge/rule.rs (`make_operator`, and the evidence bound you copy), principal/mod.rs
  (`Principal`, `expand`, `RESOLVE_SQL`), principal/cache.rs, relation/kinds.rs,
  relation/check.rs, domain/session.rs, audit.rs, event/mod.rs, dev.rs;
  crates/kernel/migrations/*.sql (never edit an applied one; add 7_authority.sql);
  crates/server/src/bootstrap.rs, auth/session.rs, sub/invalidate.rs;
  crates/api/src/commands/mod.rs and command.rs;
  ~/dev/context/procedures/engineering.md §Security §Validation.
  That is the only context you get, deliberately.

YOU OWN: crates/kernel/** except crates/kernel/src/product/**, crates/kernel/src/cmd/product.rs
  and migrations numbered 8 and above (those are P2's and P3's);
  crates/kernel/migrations/7_authority.sql;
  crates/api/src/commands/platform.rs (new). Append-only in the shared registries listed
  in docs/stories/platform-admin-legs.md §The shared registries.
DO NOT TOUCH: crates/server/src/** beyond the registry lines your commands need;
  crates/app-*/**; crates/ui/**; tools/**. You may READ everything.
  If you need something outside your paths, note it in your report — do not add it yourself.

WORK — end states, not steps.
 1. `authority::allows(&ctx, want) -> Outcome<bool>` exists and EVERY command's
    authorisation ends in it, with `platform:* #operator` as its last clause. The eight
    open-coded `is_platform_operator` calls are gone into it. Finding F1 in the
    requirements names the eighteen commands that have no operator path today; after
    this leg none do.
 2. `GrantOperator` and `RevokeOperator` (requirement A2.1): actor + mandatory bounded
    reason + audit row; `relation.granted_by` carries the granting person; revoking the
    last operator declines. `crates/kernel/src/cmd/mod.rs:361`'s claim that `SetRole`
    writes this row is corrected in the same commit.
 3. Sessions know when they were authenticated (A3.1, A3.2): `session.auth_time`,
    `OPERATOR_SESSION_TTL`, a `ReAuthenticate` command, and a named `SENSITIVE` command
    set that declines outside `PLATFORM_REAUTH_WINDOW` with a refusal a caller can
    tell apart from the uniform decline.
 4. Impersonation (C11.2): `SignInAs { person, reason }` mints a real session for an
    active identity of the target with `session.impersonated_by_identity_id` set and
    `IMPERSONATION_TTL`; `Principal::Member` carries `impersonated_by`; `refs::actor`
    returns the operator as actor and the target as hat — one function, every command;
    ten named commands are refused from an impersonated session; `EndImpersonation`
    deletes the session; the operator losing the relation ends it at resolve time.
    `RN_SITE__IMPERSONATION` gates the command (C11.3) — the config field is P5's, so
    read it through a `Ctx` flag and note the field you need in your report.
 5. The disable cascade is complete and reversible (C9.2, C9.3): `link.suspended_at`,
    stamped by `Disable`, refused by `ClaimLink`, cleared by `Enable`; the cascade
    counts land in the audit payload.
 6. `audit_object (audit_id, kind, id)` written inside every command's own transaction
    (C10.2), so an operator's ruling can be found by the object it was about without a
    full scan. Queries over it are P3's and P4's; the rows are yours.
 7. `RetireKey { kid, force?, reason }` (B6.2), refusing while tokens signed under that
    kid are alive.
 8. `ActAs` admits any organization for an operator (C11.1) — attribution only.

ACCEPTANCE: cargo fmt --check; cargo clippy --workspace --all-targets -- -D warnings;
  cargo test --workspace --locked; cargo test -p rn-kernel --test explain (every new
  lookup asserts its index); tools/size-gate.sh; cargo bench -p rn-kernel -- --quick
  (check() is still one query and still inside its budget — a leg that widens `check`
  has misread requirement C11.2).
  The two that decide the leg:
    - a table-driven test over ALL_COMMAND_NAMES: the actor holding nothing declines,
      the actor holding platform:* #operator does not, and each success wrote an audit
      row naming them. It enumerates the command list, so a command added later with no
      operator path fails the build.
    - an impersonation test asserting all six of C11.2's clauses.
  Self-test the running behaviour with tools/ephemeral.sh (`just up`), not by taking
  :3004 — that is somebody else's.
  Leave no test residue; `just down` and `pgrep -f rn-site` empty at the end.
RAILS: production and deploy are out of scope. Never print, log or commit a secret or a
  private key. Other agents are active in wave 2 on server and bundle paths — never
  `git add -A`, stage explicit files, do not revert or overwrite edits by others, and
  treat the shared registries as append-only. Do not spawn agents. Work autonomously;
  do not stop to ask questions. CARGO_BUILD_JOBS=6. Commit in units (authority sweep;
  delegation; sessions and re-auth; impersonation; cascade and audit_object; keys) and
  `git push origin rb-p1` after each.
REPORT: commit hashes; the migration, table by table; the commands added with their
  audit shapes and events; the config field you need from P5; the count of commands the
  authority table-test covers (expect 40+, and say the number); bench deltas for
  `check` and `principal::expand`; blockers.
```

---

## P2 — products at runtime

```
WORKSPACE: ~/dev/worktrees/rn-site--p2 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-p2`, cut from `rebuild` after P1 has merged). Do not touch other worktrees.
  P3 and P5 are working the same branch point in parallel; their paths are disjoint from
  yours and you must not stage a file they own.

READ FIRST, in order: docs/stories/platform-admin-requirements.md §B5 (all seven
  requirements — this leg is B5 and nothing else); the board's frames A1, A2 and F2
  (docs/design/platform-admin/, served locally); docs/rebuild/plan.md (§Product contract,
  §API); crates/server/src/lib.rs (the router, built once at boot — the whole difficulty);
  crates/server/src/sub/invalidate.rs and sub/mod.rs (the feed consumer every node runs);
  crates/server/src/state.rs; crates/kernel/src/feed/mod.rs; crates/kernel/src/relation/
  vocabulary.rs §Vocabulary (the "unregistered kind admits nothing" argument you are
  copying); crates/kernel/src/cmd/mod.rs and the authority helper P1 landed;
  crates/server/src/shell/mod.rs; tools/cluster.sh. That is the only context you get.

YOU OWN: crates/kernel/src/product/**; crates/kernel/src/cmd/product.rs;
  crates/kernel/migrations/8_product.sql; crates/server/src/product.rs;
  crates/server/src/lib.rs; crates/server/src/api/query/platform_products.rs;
  crates/api/src/commands/product.rs; crates/app-platform/src/products.rs.
  Append-only in the shared registries.
DO NOT TOUCH: any other crates/kernel/src/cmd/*.rs; any other platform query;
  crates/app-platform/src/** beyond products.rs and its one nav line; crates/ui/**;
  whoami. You may READ everything.
  If you need something outside your paths, note it in your report — do not add it yourself.

WORK — end states.
 1. A compiled-in catalogue (`rn_kernel::product::Catalogue`: slug, display, summary,
    the routes it mounts) and a `product` table carrying only enablement. A slug absent
    from the binary cannot be enabled. No row means disabled.
 2. `EnableProduct` / `DisableProduct`: operator-only, sensitive, one row each, audit
    inside, `ProductEnabled` / `ProductDisabled` events. Disabling touches no product data.
 3. Every node holds a `ProductSet` projection re-read on those two events and on nothing
    else, through the feed consumer that already runs on every node.
 4. Product routes are mounted at boot and wrapped in **one** gate that answers the
    uniform 404 when the projection says off. Do not rebuild the Router per toggle; the
    requirements doc records why, and if you disagree, say so in your report rather than
    doing it.
 5. `platform-products` and the `/platform/products` screen: slug, display, state,
    changed at, changed by, and the routes the product mounts — so the screen states
    what turning it off will take away. The toggle round-trips through /api/cmd and
    arrives as a live diff on /api/sub.
 6. `docs/rebuild/plan.md` gains the product model and the config the gate reads.

ACCEPTANCE: the CI gate (`just gate`) green. Then the one that decides the leg, and it
  is a transcript you paste into your report:
    tools/cluster.sh start                       # three real voters
    curl each node's product route               # 404, 404, 404
    POST /api/cmd/enable-product on node 1 only
    poll all three                               # 200 within 2s, on all three
    pgrep -f rn-site                             # the same three pids as before
    disable on node 3; poll all three            # 404 again
  Nothing restarted. A leg that cannot produce that transcript has not delivered B5.
  Visual: agent-browser screenshots of /platform/products at 1440 and 390, both themes,
  viewed, into docs/review/p2/. Self-test with tools/ephemeral.sh, never :3004.
  tools/cluster.sh stop; pgrep -f rn-site empty; leave no residue.
RAILS: production and deploy out of scope. Never print, log or commit secrets. P3 and P5
  are live on other paths — never `git add -A`, stage explicit files, do not revert or
  overwrite edits by others, registries are append-only. Do not spawn agents. Work
  autonomously; do not stop to ask questions. CARGO_BUILD_JOBS=6. Commit in units
  (catalogue + migration; commands; projection + gate; query + screen; docs) and
  `git push origin rb-p2` after each.
REPORT: commit hashes; the catalogue entries you shipped and why those; the three-node
  transcript in full with the pids; the gate's placement in the router and its cost per
  request; screenshot paths; blockers.
```

---

## P3 — the platform surface

```
WORKSPACE: ~/dev/worktrees/rn-site--p3 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-p3`, cut from `rebuild` after P1 has merged). Do not touch other worktrees.
  P2 and P5 are working the same branch point; their paths are disjoint from yours.

READ FIRST, in order: docs/stories/platform-admin-requirements.md §B4, §B6, §C7, §C8.2,
  §C9.3, §C10.1, §D13, §E14.2, §E15.1 — your scope, and nothing outside it; the board's
  frames F1, F3, F4 and F6 (docs/design/platform-admin/, served locally);
  ~/dev/context/design/interface-taste.md (binding — the interface states, it does not
  explain; no status pills; five type sizes); docs/rebuild/plan.md §API;
  crates/server/src/api/query/{mod,named,platform,platform_parties,platform_identities,
  platform_audit,platform_statements}.rs; crates/server/src/api/cluster.rs;
  crates/server/src/ops.rs; crates/server/src/observe.rs; crates/app-platform/src/**;
  crates/ui/src/** (read only — the table, its column priorities, and the panel);
  crates/kernel/src/oidc/{key,token,client}.rs and migration 6_oidc.sql;
  crates/server/src/api/whoami/name.rs (`masked` — the address rule you must obey).
  That is the only context you get.

YOU OWN: crates/server/src/api/query/platform_*.rs except platform_products.rs;
  crates/server/src/api/cluster.rs; crates/server/src/observe.rs;
  crates/server/src/ops.rs; crates/kernel/migrations/9_node_report.sql;
  crates/api/src/cluster.rs; crates/app-platform/src/** except products.rs.
  Append-only in the shared registries.
DO NOT TOUCH: crates/kernel/src beyond your migration; crates/ui/**; crates/app-member/**;
  whoami; crates/server/src/lib.rs. You may READ everything.
  If you need something outside your paths, note it in your report — do not add it yourself.

WORK — end states.
 1. `node_report`, upserted by each node from the observation lane every 15 s, and the
    `platform-nodes` query over it with a rollout verdict *counted from the rows*
    (settled / in progress / divergent). A row older than three intervals renders as
    "not reporting" — the screen says what it witnessed.
 2. The deployment screen (frame F1): the node table, both raft groups, feed head and
    commands-in-the-last-day as two readings, subscribers, pending observations, the key
    table with ages, and rotate. `/platform/cluster` becomes a section of it.
 3. `platform-keys` with live-token counts, `platform-clients` with last issuance,
    `platform-links` and `platform-consents` with revoke and a bulk revoke by client.
 4. The person page (frame F3): identities, factors **masked**, sessions, memberships,
    owned resources, relations, consents, merge history with the signals a candidate
    actually carries, and the controls. `reveal-factor` writes its own audit row.
 5. `platform-find?q=` — exact handle, exact email, exact public id, three seeks, no
    prefix scan. A miss is empty, not a decline.
 6. Audit filters (frame F4) by actor, hat, object, command and time, each riding an
    index, over P1's `audit_object` side table where the filter is by object.
 7. `/platform/operators` renders P1's rows, including the bootstrap grant with its
    null `granted_by` stated as "granted by configuration".

ACCEPTANCE: `just gate` green; `cargo test -p rn-site --test explain` covers every new
  statement (and `platform_statements.rs` lists them, so the completeness test still
  passes); a generated test asserts every filter combination the controls can produce is
  an index seek. A server test asserts the rendered person JSON contains no full email
  address. Visual: agent-browser screenshots of every page you touched at 1440 and 390,
  both themes, VIEWED (not merely captured), into docs/review/p3/ — including the
  deployment screen with three voters up and again with one killed
  (tools/cluster.sh kill 2). Self-test with tools/ephemeral.sh, never :3004.
  tools/cluster.sh stop; no residue; pgrep -f rn-site empty.
RAILS: production and deploy out of scope. Never print, log or commit secrets, and never
  put an unmasked address in a screenshot. P2 and P5 are live on other paths — never
  `git add -A`, stage explicit files, do not revert or overwrite edits by others,
  registries are append-only. Do not spawn agents. Work autonomously; do not stop to ask
  questions. CARGO_BUILD_JOBS=6. Commit in units (node_report + query; deployment screen;
  keys and clients; links and consents; person page; find; audit filters) and
  `git push origin rb-p3` after each.
REPORT: commit hashes; the queries added with their indexes; the EXPLAIN output for the
  three `platform-find` statements; screenshot paths, and which ones you looked at;
  anything the interface-taste rules made you undo; blockers.
```

---

## P5 — the way in without the web tier

```
WORKSPACE: ~/dev/worktrees/rn-site--p5 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-p5`, cut from `rebuild` after P1 has merged). Do not touch other worktrees.
  P2 and P3 are working the same branch point; their paths are disjoint from yours.

READ FIRST, in order: docs/stories/platform-admin-requirements.md §F16, §F18, §G21, and
  §C11.3 (the config field P1 asked for); the board's frame E1
  (docs/design/platform-admin/, served locally); crates/server/src/main.rs (the
  subcommand shape); crates/server/src/bootstrap.rs (why a formed cluster cannot run a
  subcommand — the whole reason G21 has two halves); crates/server/src/config.rs;
  crates/server/src/db/{mod,cluster,migrations}.rs; crates/server/src/lib.rs (read only —
  P2 owns it); crates/kernel/src/store/mod.rs and store/replicated.rs;
  crates/kernel/src/cmd/mod.rs (you run the real commands, never a second authorisation
  path); tools/{cluster,seed,ephemeral}.sh; deploy/CUTOVER.md; and hiqlite 0.14's own
  documentation, fetched, not from memory. That is the only context you get.

YOU OWN: crates/server/src/admin/**; crates/server/src/main.rs; crates/server/src/config.rs;
  tools/cluster.sh; tools/ephemeral.sh; docs/ops-backup.md; README.md (P5 is the only leg
  that writes it — every other leg notes its README line in its report instead).
DO NOT TOUCH: crates/kernel/**; crates/server/src/lib.rs; any query; any bundle.
  You may READ everything.
  If you need something outside your paths, note it in your report — do not add it yourself.

WORK — end states.
 1. **First commit is a finding, not a design.** `docs/ops-backup.md` states what
    hiqlite 0.14 actually exposes for snapshot and restore, with the doc link and the
    version pinned, or states that it exposes nothing. Everything after depends on which.
 2. `rn-site admin <cmd>` against a stopped node's store: operators, grant-operator,
    revoke-operator, sessions, revoke-session, products, enable, disable, audit --tail,
    backup, restore, wipe. Each runs the same kernel command the API runs, with a
    principal built from `--as <email>`, so every action lands in the audit with an
    actor. Against a running node it exits non-zero **naming the pid holding the lock**.
 3. Backup and restore (F16.2, F16.3): a manifest carrying the offset it is consistent
    at, the schema hash, the id-key fingerprint and per-table counts; a restore that
    refuses a non-empty database and refuses an id-key mismatch rather than silently
    renaming every object; `/readyz` reporting the restored offset as the feed head.
 4. `RN_SITE__ADMIN_ADDR` opens a second, **loopback-only** listener serving `/admin/*`
    with the same actions on a *running* node. The config refuses a non-loopback address
    at boot, in both modes. Reaching the socket is the authority, and that is defensible
    only because it is loopback — write that sentence in the module doc.
 5. `RN_SITE__IMPERSONATION` (C11.3), the field P1 asked for.
 6. `tools/cluster.sh reset` (stop, wipe, start, seed) and `tools/ephemeral.sh
    backup|restore` wrapping the two subcommands.

ACCEPTANCE: `just gate` green; `bash -n tools/*.sh`. A route-matrix row asserts `/admin/*`
  is absent from the public router for all six principals. A config test asserts a
  non-loopback `RN_SITE__ADMIN_ADDR` fails validation. The round trip as a script and a
  transcript: an ephemeral instance seeded with a person, an organization and a document;
  backup; `reset`; restore; every public id resolves to the same rows. `admin
  grant-operator` against a stopped instance writes a relation row **and** an audit row;
  against a running one it names the lock. Self-test with tools/ephemeral.sh, never :3004.
  No residue; `pgrep -f rn-site` empty.
RAILS: production and deploy out of scope — this leg builds the tools, it does not run
  them anywhere but a local ephemeral instance. Never print, log or commit secrets, and
  the backup manifest carries an id-key **fingerprint**, never the key. P2 and P3 are
  live on other paths — never `git add -A`, stage explicit files, do not revert or
  overwrite edits by others. Do not spawn agents. Work autonomously; do not stop to ask
  questions. CARGO_BUILD_JOBS=6. Commit in units (the hiqlite finding; the CLI; backup
  and restore; the admin listener; the config fields; the scripts) and
  `git push origin rb-p5` after each.
REPORT: commit hashes; what hiqlite 0.14 exposes, verbatim, with the link; the
  subcommand list; the config keys added; the backup round-trip transcript; whether the
  logical walk or a hiqlite snapshot is what shipped, and why; blockers.
```

---

## P4 — what the person sees

```
WORKSPACE: ~/dev/worktrees/rn-site--p4 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-p4`, cut from `rebuild` after P1, P2 and P3 have merged). You are the only
  worker on this branch point. Do not touch other worktrees.

READ FIRST, in order: docs/stories/platform-admin-requirements.md §C10.2, §C11.2 (the
  banner), §B5.5, §E15.2; the board's frames B2, F5 and F6 (docs/design/platform-admin/,
  served locally); ~/dev/context/design/interface-taste.md (binding: no status pills, no
  explanatory quips, larger never heavier — the bar is a bar, not a toast and not a
  tinted capsule); crates/server/src/api/whoami/mod.rs (and its "what is NOT here is the
  point" doc, plus `whoami_leaks_nothing`); crates/server/src/api/query/{named,member_*}.rs;
  crates/ui/src/** (the shell, the rail and drawer, the table); crates/app-member/src/**;
  P1's `audit_object` table and `impersonated_by` column; P2's product set.
  That is the only context you get.

YOU OWN: crates/server/src/api/whoami/**; crates/server/src/api/query/member_*.rs and the
  new about_me query; crates/ui/src/**; crates/app-member/src/**;
  end2end/tests/impersonation.spec.ts. Append-only in the shared registries.
DO NOT TOUCH: crates/kernel/**; crates/app-platform/**; crates/app-org/**;
  crates/server/src beyond whoami and your queries. You may READ everything.
  If you need something outside your paths, note it in your report — do not add it yourself.

WORK — end states.
 1. `about-me` (C10.2): audit rows whose `acting_as` is my person **or** whose
    `audit_object` rows name something I hold, whoever the actor was, with the actor's
    display name. It is a different question from `audit` and both stay. Two index seeks
    and a union, never a JSON scan.
 2. `/app` shows it. A person can see that an operator revoked their session, ruled on
    their merge, or signed in as them, and by whom.
 3. `whoami` gains `impersonating: { display, since }` and `products: [slug]`, and
    `whoami_leaks_nothing` is extended rather than relaxed.
 4. The shell draws the impersonation bar (frame F5): persistent, full sentence, an
    *end* control, and it survives the drawer at 390. Not a toast, not a pill.
 5. The rail omits a disabled product's item.
 6. `end2end/tests/impersonation.spec.ts` proves all six clauses of C11.2 through the
    browser, in two contexts.

ACCEPTANCE: `just gate` green; `just e2e` green including your new spec, against an
  instance seeded by tools/seed.sh; `cargo test -p rn-site --test explain` covers the
  about-me statements. Visual: agent-browser screenshots of /app with and without the
  bar, at 1440 and 390, both themes, VIEWED, into docs/review/p4/. Self-test with
  tools/ephemeral.sh, never :3004. No residue; `pgrep -f rn-site` empty.
RAILS: production and deploy out of scope. Never print, log or commit secrets. Never
  widen whoami without a screen that renders the new field — that is what the leak test
  is for. Never `git add -A`, stage explicit files. Do not spawn agents. Work
  autonomously; do not stop to ask questions. CARGO_BUILD_JOBS=6. Commit in units
  (about-me; the /app page; whoami; the bar; the e2e spec) and `git push origin rb-p4`
  after each.
REPORT: commit hashes; the about-me query plan; what you added to whoami and which
  screen renders it; screenshot paths and which you looked at; the e2e spec's six
  assertions and their results; blockers.
```

---

## V1 — validation (Sonnet, dispatched by the orchestrator)

```
WORKSPACE: ~/dev/worktrees/rn-site--v1 (git worktree of ~/dev/love/projects/ronit/rn-site,
  branch `rb-v1`, cut from `rebuild` after P1–P5 have merged). Do not touch other
  worktrees. You write findings, not fixes.

READ FIRST, in order: docs/stories/platform-admin.md (the 21 stories — this is what you
  are checking, not the code); docs/stories/platform-admin-requirements.md (the
  acceptance test named under each requirement); ~/dev/context/design/interface-taste.md
  and ~/dev/context/design/ui-ux-heuristics.md (binding — you review against these);
  docs/design/platform-admin/dist/index.html served locally, for what the screens were
  meant to be; README.md §Working on it. Read no Rust unless a finding needs a file:line.
  That is the only context you get, deliberately: you are the reader who was not in the
  room.

YOU OWN: docs/review/platform-admin/** (your findings and your screenshots). Nothing else.
DO NOT TOUCH: any crate, any tool, any other doc. You may READ everything.

WORK — a human-style walkthrough, then a critique.
 1. `just up` (tools/ephemeral.sh — your own ports, never :3004). Sign in with the
    operator button. Then walk all 21 stories in order as a person would, with
    agent-browser, at 1440 and at 390, in both themes:
    become the operator, delegate to a second person and take it back, read the
    deployment screen while `tools/cluster.sh` has three voters and again with one
    killed, turn a product off and confirm its route is gone from a second node without
    a restart, rotate a signing key, find a person three ways, rule on a merge, disable
    and re-enable somebody and check that their invitation link died and came back,
    revoke a session and confirm the owner can see that you did, sign in as somebody and
    confirm the bar, the audit row and their own record of it, transfer something, filter
    the audit five ways, register and delete a relying party, back up and restore,
    wipe and rebootstrap, and get in over the admin socket with the web tier answering.
 2. `docs/review/platform-admin/index.html` — a served static report (never a Claude
    artifact): one row per story, PASS / PARTIAL / FAIL with the evidence, the
    screenshot, and for a FAIL the exact reproduction. Where the requirements doc names
    a falsifiable test, run it and record its output verbatim.
 3. A UX critique, separately, against interface-taste: what explains rather than
    states, what is a tinted capsule instead of a word, what reads as an AI-built
    interface, what a real operator would fail to find. Be specific and be willing to
    say a screen is fine.
 4. Anything you could not exercise, listed as such. A story you did not reach is not
    a PASS.

ACCEPTANCE: all 21 stories walked or explicitly listed as unreachable; every screenshot
  VIEWED, not merely captured (a curl cannot see a broken layout); the report served and
  its URL in your report; `just down`; `tools/cluster.sh stop`;
  `pgrep -f 'rn-site|http.server'` empty; no residue; nothing outside
  docs/review/platform-admin/ staged.
RAILS: you fix nothing — a finding with a repro is worth more than a patch nobody
  reviewed. Production and deploy out of scope. Never print, log or commit secrets, and
  never screenshot an unmasked address. Do not kill a process you did not start: check
  its pid, command line and start time first. Never `git add -A`. Do not spawn agents.
  Work autonomously; do not stop to ask questions.
REPORT: the commit hash of your findings; the PASS / PARTIAL / FAIL count over 21 —
  say the three numbers; the three worst findings with their repros; the UX critique's
  headline; the report URL and the screenshot directory; what you could not exercise.
```
