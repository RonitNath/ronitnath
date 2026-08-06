---
product: ronitnath-universe
bet: events platform + identity model on the universe platform
date: 2026-08-05
interviewer: claude (this session)
status: interview complete — appetite number pending confirmation at pitch freeze
confidence: high
---

## Agenda coverage

| # | Topic | Status | Distillation |
| --- | --- | --- | --- |
| A1 | Problem | **covered** | Site dormant; historical mode was one codebase per event (EV-013). No self-serve path to create an event ("pickleball Aug 15"). Want a CMS-style create/modify/manage interface, then make it shareable. (EV-008) |
| A2 | Users | **covered** | Friends get accounts; long-term ronitnath.com = hub + private social platform. Agents get NO identities — machine edits via MCP/HTTP. (EV-009) |
| A3 | Bet shape & bound | **covered (bound pending)** | One bet ("ok" to interviewer's read): event platform + minimum identity under it. Owner rejected time-denominated appetite (→ SYS-DEC-001); proposed bound: **3 owner review sessions**, optionally pinned to a real event date, confirmed at pitch freeze. |
| A4 | Constraints | **covered** | Data export + import required. Backup of past three events (friends, attendance) is the one hard data constraint — analytics, future accounts, future invites. Postgres seam + isoastra-separate accounts stand (EV-002). |
| A5 | No-gos | **covered** | Calendar, circles, photos wait until event CMS is easy + shareable. Photos vision recorded for a later bet (EV-010). Flexibility + per-event style identity are IN — resolved OQ-2 (EV-011, EV-013). |
| A6 | Success signals | **covered** | Revised pickleball test: owner tells agent about the event → agent creates it via API → owner tweaks wording + manages invites in UI → shares link → friend RSVPs — **on the real public ronitnath.com** (EV-012). |
| A7 | Risks / irreversibility | **covered** | Owner: "nothing's expensive" except the past-events data, which backup + export/import covers. Calculus flips once friends hold accounts. |

## Answers

- **Problem**: EV-008. The general system replaces one-off validated deployments; the first capability is arbitrary event creation/management through a UI.
- **Users**: EV-009. Host (owner) now; friends as account-holders as the hub grows. No agent identities — MCP/HTTP editing surface instead.
- **Constraints**: export/import as first-class; three past events' people+attendance data preserved and importable; PostgreSQL seam; account population separate from isoastra (EV-002).
- **No-gos this bet**: calendar, circles, photos, poster theming, E2EE. Telegram unresolved but non-blocking (OQ-4).
- **Risk posture**: low irreversibility while no friends hold accounts; the calculus flips once accounts exist.

## Open questions

- **OQ-1** — ~~console-shell ordering~~ **Resolved by owner direction**: the event-CMS bet subsumes the base-cut tail; DEC-003's presence-first lesson stands, sequencing abstraction does not override the concrete want.
- **OQ-2** — ~~fixed template vs modular~~ **Resolved by owner + artifact evidence**: the three past events genuinely differed on five axes (EV-013) and per-event style identity is load-bearing (EV-011). Flexibility is the point; a fixed template repeats the failure that forced three rebuilds. Modular composition (EV-006 direction) is in scope; *how much* module surface ships in cut one is a pitch/debate question, not an interview question.
- **OQ-3** — ~~legacy import scope~~ **Resolved**: export/import is a required capability; past-three-events data must be backed up and importable. Full parity migration NOT required.
- **OQ-4** — Telegram host pings for parity? *Blocking: no. Deferred.*
- **OQ-5** — Agent MCP/HTTP editing: what does it authenticate as (owner API token? scoped capability token)? *Blocking: no — work-packet detail, but shapes the API surface.*
- **OQ-6** — Do the 38 legacy `/e/{token}` invite URLs need to keep resolving after the new platform takes over? Site is dormant, so likely moot. *Blocking: no.*

## Debate pre-check (final)

`debate: triggered (T3)` — two schema commitments outlive the bet and get expensive exactly when the bet succeeds (friends onboard, links shared publicly): (1) the minimum identity schema + capability-link model, (2) the event page-doc/module schema that all future events' content lives in. Debate scope is those two models plus the agent-API surface (OQ-5); everything presentational is out of debate scope. Open questions OQ-4/5/6 are inherited by the debate.
