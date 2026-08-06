---
product: ronitnath-universe
bet: events platform + identity model on the universe platform
date: 2026-08-05
interviewer: claude (this session)
status: draft — agenda awaiting strike/add
confidence: —
---

## Agenda (pitch altitude — shown for strike/add before questioning)

| # | Topic | Angle for this bet | Leans on |
| --- | --- | --- | --- |
| A1 | Problem | Live site already does events well and keeps serving. Why is rebuilding events+identity the next bet *now* — parity-for-cutover, or new capability the live site can't grow? What hurts today? | EV-001 |
| A2 | Users | Guests, you-as-host, and — new question the universe raises — does anyone else *ever* hold an account? Do agents get identities here? | EV-005 |
| A3 | Bet shape & appetite | One bet or two (identity as its own bet vs. infra rider on events)? Fixed time budget for each. | EV-003 |
| A4 | Constraints | Data migration from live sqlite (dup people rows included?); invite URLs keep resolving; PostgreSQL seam; parity-before-cutover?; separate account population from isoastra. | EV-001, EV-002 |
| A5 | No-gos | Modular page composition in or out of this bet? Telegram pings? Photos, calendar, circles — riders or later bets? Poster theming, E2EE. | EV-006, EV-007 |
| A6 | Success signals | The observable "won" event — e.g. one real event hosted end-to-end on the new platform, or full ronitnath.com cutover? | EV-001 |
| A7 | Risks / irreversibility | Identity schema on Postgres, encryption-key custody, universal-entity model — all T3 debate material. What's *actually* expensive to walk back vs. cheap? | EV-004, EV-005 |

## Answers

(pending interview)

## Open questions (pre-logged from context; interview or debate resolves)

- **OQ-1** — Base-cut order (DEC-003) says identity → console shell before events; this bet pairs events+identity directly. Does console shell defer, or does the order stand and this bet subsumes it? *Blocking: yes (sequencing).*
- **OQ-2** — Is modular page composition (EV-006) in this bet's scope, or is fixed-page parity enough for the first cut? *Blocking: yes (scope size differs ~2×).*
- **OQ-3** — Is legacy-data import (people dedup, malformed names) in scope, or does the new platform start clean and import at cutover? *Blocking: no (deferrable).*
- **OQ-4** — Are Telegram host pings (EV-007) required for parity? *Blocking: no.*

## Debate pre-check (per system/debate-triggers.md)

T3 already fires regardless of interview outcome: identity data model + ID-encryption key custody are schema/auth commitments on stored user data. Expect `debate: triggered (T3)` after the pitch draft.
