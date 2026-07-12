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
| 0 | Bootstrap (repo, two-bin split) | phase-0-bootstrap.md | in progress |
| 1 | Design brief + token port | phase-1-design.md | pending |
| 2 | Domain port from events | phase-2-domain-port.md | pending |
| 3 | Visibility (circles + levels) | phase-3-visibility.md | pending |
| 4 | Guest accounts (claim + password) | phase-4-guest-accounts.md | pending |
| 5 | Photos | phase-5-photos.md | pending |
| 6 | Calendar | phase-6-calendar.md | pending |
| 7 | Prod data migration (dry run) | phase-7-import.md | pending |
| 8 | Cutover (operator, by hand) | phase-8-cutover.md | pending |

Phases are sequential; each ends with a gate review (tests, acceptance
evidence, worktree clean) before the next is dispatched.

## Work ledger

| Phase | Leg | Worker | Model/effort | Job/commit | Evidence |
|-------|-----|--------|--------------|------------|----------|
| 0 | two-bin split | pi_codex | gpt-5.6-sol / medium | job 7663a885-a899-443d-aed9-083ecb7f8012 | pending |
