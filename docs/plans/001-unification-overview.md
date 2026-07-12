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
| 4 | Guest accounts (claim + password) | phase-4-guest-accounts.md | done |
| 5 | Photos | phase-5-photos.md | done |
| 6 | Calendar | phase-6-calendar.md | done |
| 7 | Prod data migration (dry run) | phase-7-import.md | done |
| 8 | Cutover (operator, by hand) | phase-8-cutover.md | ready — awaiting operator go |

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
| 4 | guest accounts build | pi_codex | gpt-5.6-sol / high | job 6b93d348 → 77967bb, 8610f51, 2bd2072 | 77/77 tests; live claim→logout→login→/my→session-RSVP→revoke transcript; rulings appended to Amendments (recovery-email login fails-closed w/ dummy verify; /my/events floors only with own live link; claimed links 404). |
| 4 | independent security review | pi_codex (read-only, detached @2bd2072) | gpt-5.6-sol / high | job d3348b19 | Verdict FAIL: BLOCKER (mismatch-session RSVP attributed to token person, no CSRF) + 3 SHOULD-FIX (claim race 500s, guest login unscoped by account, force-unlink session redirect-loop). Claim capability/GuestScope isolation/oracle/cookie flags clean. |
| 4 | security fix round | pi_codex | gpt-5.6-sol / high | job 4b0b1c85 → 0f98701 | 81/81 tests (orchestrator re-ran + read the CSRF/attribution branch); session-guest RSVP: CSRF required, writes attribute to session person, render stays token-scoped; BEGIN IMMEDIATE claims → race maps to 4xx; login lookup owner-account-scoped; force-unlink revokes all identity sessions, /logout works on any live session; 5 reviewer-named regression tests added. |
| 5 | photos build | pi_codex | gpt-5.6-sol / high | job ffd2e2f8 → 8264ab9, a5cf585, 5e677c6 | 84/84 tests; EXIF-safe WebP pipeline, attendee-gated serve/upload/delete, content-hash dedup, photos-gc; live upload/variant/404 curl transcript. |
| 5 | independent security review | pi_codex (read-only, detached @5e677c6) | gpt-5.6-sol / high | job cc87256a | Verdict FAIL: BLOCKER (unbounded decode = decompression-bomb DoS on public upload) + 4 SHOULD-FIX (GC/upload race, substring body-limit bypass, EXIF UTF-8 panic, raw filename). Path safety/authz/EXIF-strip/CSRF/serve-type clean. |
| 5 | security fix round | pi_codex | gpt-5.6-sol / high | job 4ae155af → 6a2606f | 89/89 tests (orchestrator re-ran + read caps/spawn_blocking); decode capped 8192²/50MP + spawn_blocking + 2-permit semaphore; atomic temp+rename with writer-lock-serialized GC; structural photo/non-photo router split; ASCII-validated EXIF; filename sanitized 255B. |
| 5 | gallery visual gate + polish | orchestrator (agent-browser) + self | — | 48cdba4 | Live gallery/lightbox/upload/dedup verified by orchestrator (2 real JPEGs, variants served, non-attendee 404, dedup 3 rows/2 keys). One completeness gap fixed: ::file-selector-button tokenized (native date/checkbox chrome stays per documented base.css decision). |
| 6 | calendar build | pi_codex → codex exec | gpt-5.6-sol/high (WebSocket drop ~38min) → codex exec gpt-5.5/high finished | 84dcac7, a6a1d52, a479758 | 94/94 tests (orchestrator re-ran offline, no DATABASE_URL); read-time union via level_for (Viewer::FeedHolder, no floor); CalendarEntry::view_for 4th chokepoint; live: anon /calendar shows July4 summary-only chip, ICS feed redacts (busy→SUMMARY:Busy, summary→title-only, full→both), revoke→404. Visual gate passed (orchestrator screenshot). |
| 6 | independent security review | pi_codex (read-only, detached @a479758) | gpt-5.6-sol / high | job 0838540c | Verdict FAIL: 2 BLOCKER (capability tokens logged plaintext in telemetry span — affects /e/{token} + photo paths from phases 2-5, not just calendar; extreme-month panic) + 3 SHOULD-FIX (feed revoke race + no cache headers, partial-commit audience save, missing ICS line-folding). Token routing/visibility/tenant-isolation/ICS-injection/admin-lifecycle clean. |
| 6 | security fix round | pi_codex | gpt-5.6-sol / high | job 13d4f9ef → 2d449a5 | 98/98 tests offline (orchestrator re-ran); telemetry path sanitizer (orchestrator live-verified: sentinel token 0 hits in logs, paths log as /e/{token} + /calendar/{feed}.ics — retroactively fixes phases 2-5 token leak); month range 1970-2100 checked-arith; feed touch conditional-on-unrevoked → 404 + private,no-store; audience save validates-all-then-txn+audit (event editor shared bug, fixed once); ICS 75-octet folding on char boundaries. |
| 7 | prod import CLIs + dry run | pi_codex | gpt-5.6-sol / high | job 6bd891f2 → 60f419e | 99/99 tests; import-legacy-db (empty-guard, PK-verbatim, single-txn) + verify-import (16-check PASS table). ORCHESTRATOR INDEPENDENT DRY RUN on real prod snapshot (90 people/3 events/38 links, .backup from live nexus): re-migrated fresh v33 DB + re-imported + re-verified (all PASS); resolved ALL 38 real tokens (36 live→200, 2 revoked→404, bogus→404); person-bound link shows 1 Pine St (Full backfill); PK spot-check people+event_links src==imported 0 mismatches. v21→v33 gap = accounts.purpose='primary' + people.recovery_email NULL only. Note: pre-existing malformed people.name junk in live data (not import artifact). |
