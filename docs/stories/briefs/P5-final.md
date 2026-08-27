ORCHESTRATOR NOTE: the worktree exists at /Users/ronitnath/dev/worktrees/rn-site--p5 (branch rb-p5, cut from rebuild after P1 merged — P1 landed: authority::allows in every command, GrantOperator/RevokeOperator, ReAuthenticate + OPERATOR_SESSION_TTL + auth_time, SignInAs/EndImpersonation (bound `linking`: the session secret rides in the reply body; browser adoption is P4), link.suspended_at, audit_object rows, RetireKey; 47 commands; migration 7_authority.sql; Ctx::impersonation is currently filled from `mode == Dev` in crates/server/src/api/cmd/mod.rs — P5 replaces that with RN_SITE__IMPERSONATION). A prebuilt target/ is being copied in; DO NOT run cargo/trunk/just until target/.seed-complete exists (read first — there is a lot to read). P2, P3 and P5 run concurrently: stay inside YOU OWN; shared registries are append-only (/private/tmp/claude-502/-Users-ronitnath-dev/1a80dd12-969c-454a-80ea-f8f4533b4fa6/scratchpad/briefs/registries.md). Your migration number: P2 = 8, P3 = 9 (do not renumber). CARGO_BUILD_JOBS=4 (three workers share the machine).


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

---

