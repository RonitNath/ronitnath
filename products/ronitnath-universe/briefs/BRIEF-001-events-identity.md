---
product: ronitnath-universe
bet: events platform + identity model on the universe platform
date: 2026-08-05
interviewer: claude (this session)
status: draft — interview in progress (A3 uncovered; A5/A6 confirmations pending)
confidence: —
---

## Agenda coverage

| # | Topic | Status | Distillation |
| --- | --- | --- | --- |
| A1 | Problem | **covered** | Site dormant; historical mode was narrow per-event deployments. No self-serve path to create an event ("pickleball Aug 15"). Want a CMS-style create/modify/manage interface, then make it shareable. (EV-008) |
| A2 | Users | **covered** | Friends get accounts; long-term ronitnath.com = hub + private social platform. Agents get NO identities — machine edits via MCP/HTTP. (EV-009) |
| A3 | Bet shape & appetite | **open** | Owner asked what this means — explained; answer pending. |
| A4 | Constraints | **covered** | Data export + import required. Backup of past three events (friends, attendance) is the one hard data constraint — analytics, future accounts, future invites. Postgres seam + isoastra-separate accounts stand (EV-002). |
| A5 | No-gos | **mostly covered** | Calendar, circles, photos wait until event CMS is easy + shareable. Photos vision recorded for a later bet (EV-010). Pending: fixed event template vs modular page composition for the first cut (OQ-2). |
| A6 | Success signals | **implicit, confirming** | "Easy to do and share with others" → proposed pickleball test (see below). |
| A7 | Risks / irreversibility | **covered** | Owner: "nothing's expensive" except the past-events data, which backup + export/import covers. |

## Answers

- **Problem**: EV-008. The general system replaces one-off validated deployments; the first capability is arbitrary event creation/management through a UI.
- **Users**: EV-009. Host (owner) now; friends as account-holders as the hub grows. No agent identities — MCP/HTTP editing surface instead.
- **Constraints**: export/import as first-class; three past events' people+attendance data preserved and importable; PostgreSQL seam; account population separate from isoastra (EV-002).
- **No-gos this bet**: calendar, circles, photos, poster theming, E2EE. Telegram unresolved but non-blocking (OQ-4).
- **Risk posture**: low irreversibility while no friends hold accounts; the calculus flips once accounts exist.

## Open questions

- **OQ-1** — ~~console-shell ordering~~ **Resolved by owner direction**: the event-CMS bet subsumes the base-cut tail; DEC-003's presence-first lesson stands, sequencing abstraction does not override the concrete want.
- **OQ-2** — First cut: fixed event template (title/time/venue/body/RSVP) vs modular page composition (EV-006)? *Blocking: yes — asked in interview.*
- **OQ-3** — ~~legacy import scope~~ **Resolved**: export/import is a required capability; past-three-events data must be backed up and importable. Full parity migration NOT required.
- **OQ-4** — Telegram host pings for parity? *Blocking: no. Deferred.*
- **OQ-5** — Agent MCP/HTTP editing: what does it authenticate as (owner API token? scoped capability token)? *Blocking: no — work-packet detail, but shapes the API surface.*
- **OQ-6** — Do the 38 legacy `/e/{token}` invite URLs need to keep resolving after the new platform takes over? Site is dormant, so likely moot. *Blocking: no.*

## Debate pre-check (revised)

T3 originally assumed expensive-to-reverse identity commitments. Owner ground truth says nothing is expensive while no friends hold accounts and export/import exists. T3 therefore hinges on one question: is the identity model cheap to revise *after* friends onboard? Decide at pitch time; likely outcome is a scoped debate on the identity schema only, or a sanity pass if appetite is small (T2 pending A3 answer).
