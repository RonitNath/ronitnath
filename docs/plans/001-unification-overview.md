# ronitnath.com unification — plan overview

Fresh fork of stage_2 @ a126c64 becomes the single app behind ronitnath.com,
absorbing the events subdomain (RonitNath/events, live on nexus :3117/:3118)
and the legacy site (RonitNath/ronitnath-legacy, T2 gateway). Architecture
contract: `000-architecture.md` (circles/levels, viewer model, claim flow,
photos, calendar). Design contract: `docs/design.md` (phase 1).

Two bins: `site` (public, :3130) + `admin` (NetBird-mesh-only, :3131).
Events keeps :3117/:3118 until cutover; both run side by side on nexus.

## Phase status

| # | Phase | File | Status |
|---|-------|------|--------|
| 0 | Bootstrap (repo, two-bin split) | phase-0-bootstrap.md | done |
| 1 | Design brief + token port | phase-1-design.md | done |
| 2 | Domain port from events | phase-2-domain-port.md | done |
| 3 | Visibility (circles + levels) | phase-3-visibility.md | done |
| 4 | Guest accounts (claim + password) | phase-4-guest-accounts.md | in progress |
| 5 | Photos | phase-5-photos.md | pending |
| 6 | Calendar | phase-6-calendar.md | pending |
| 7 | Prod data migration (dry run) | phase-7-import.md | pending |
| 8 | Cutover (operator, by hand) | phase-8-cutover.md | pending |

Phases are sequential; each ends with a gate review (tests, acceptance
evidence, worktree clean) before the next is dispatched.

## Work ledger

| Phase | Leg | Worker | Model/effort | Job/commit | Evidence |
|-------|-----|--------|--------------|------------|----------|
| 0 | two-bin split | pi_codex | gpt-5.6-sol / medium | job 7663a885 → 36f98c1, 635beb9 | 26/26 tests (orchestrator re-ran); both bins booted, healthz ok; admin /login 200, site /login 404; sqlx cache committed; docker unavailable on mu — compose validated at deploy. Deviation (accepted): site owns migrations, admin fails fast on stale schema (connect_existing). |
| 1 | design brief + token port | claude sonnet | sonnet / default | 9334ba6 | Brief + tokens + atmosphere + home/auth restyle; 2 real bugs fixed (CRLF→CSP hash normalization — upstream to stage_2; :user-invalid). Screenshots vs legacy at 360-1280 dark+light, viewed by orchestrator. Deviations accepted: 768px mobile breakpoint, localStorage theme kept, static/css/ paths. |
| 1 | gate fix round | pi_codex | gpt-5.6-sol / medium (luna broken via MCP — memory) | job 4b1ef265 → 0485a2e | 27/27 tests incl. new site_home_does_not_advertise_auth; orchestrator live-verified: site-bin HTML has zero /login//signup refs, nav = Home/About + monochrome SVG toggle, both themes screenshot-verified. |
| 2 | domain port | pi_codex | gpt-5.6-sol / high | job a66eae78 → 9a61225, 8260576, 930e375, 3729d1a | 56/56 tests (orchestrator re-ran); live smoke on scratch DB: seed recovered 39+46 guests + July 4th, public link 200 w/o address, private shows 1 Pine St + entry images, bogus 404, browser RSVP roundtrip persisted person+attendance+auto-minted personal link, ICS via /events/{token}/ics tier-redacted. Gate fixes (orchestrator, 6163385+1c13a69): static/img port missed; Saira Condensed restored on poster; bare-anchor token rule (phase-1 audit miss). |
| 3 | visibility build | pi_codex ×2 + codex exec | gpt-5.6-sol / high ×2 (both died: provider_transport_failure ~7.7min) → codex exec gpt-5.5/high finished | bf7564a | 69/69 tests; contract Amendments (direct-hit tier floor; 0027 renumber). Worker correctly refused my contradictory busy-curl instruction (public link floors Summary). |
| 3 | independent security review | pi_codex (read-only, detached worktree) | gpt-5.6-sol / high | job ab761dff | Verdict FAIL: 1 BLOCKER (RSVP JSON leaked private segment IDs/counts at Summary), 2 SHOULD-FIX (silent cross-account circle no-ops + missing tests). IDOR/CSRF/chokepoints/semantics otherwise clean. |
| 3 | security fix round | pi_codex | gpt-5.6-sol / high | job c6fd36f3 → 74413d4 | 73/73 tests (orchestrator re-ran); segment_counts + list_segment_rsvps_for_person now Level-aware at store chokepoint (SQL: >=Summary for any, =Full for private — orchestrator read the queries); circle membership 404s on zero rows, audits only real mutations; admin passes Full explicitly. |
