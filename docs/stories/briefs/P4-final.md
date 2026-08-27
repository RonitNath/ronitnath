ORCHESTRATOR NOTE (placeholder — the worktree path and cut point are filled in at dispatch, after P2/P3/P5 merge). Handoffs from P1 that are yours: (1) `SignInAs` is bound `linking` — the impersonation session secret comes back in the reply body; the browser must adopt it as a SECOND context (a separate tab/window state, never replacing the operator cookie) and show the impersonation banner in the impersonated person's own view, with EndImpersonation ending that context only; (2) `Impersonated`/`ImpersonationEnded` events do not push a live diff to the platform sessions list unless P3 fixed it — check; (3) P1 wrote the granting IDENTITY into relation.granted_by — resolve display names through identity.person_id.


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

---

