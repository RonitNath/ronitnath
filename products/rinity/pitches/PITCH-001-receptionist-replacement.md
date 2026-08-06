---
product: rinity
pitch: PITCH-001 — replace the receptionist (tier 1)
date: 2026-08-06
status: DRAFT — awaiting owner review; carries one debate escalation (E-1, RCDA go-live sequencing)
sources: BRIEF-001, FLOWS-001, DEBATE-001
debate: triggered (T3: PHI/call-data retention, customer identity model, self-serve URL + config-schema commitments, the rinity↔audgent seam; + T2: bound waived, EV-014) → memo DEBATE-001
---

## Problem

A dental practice pays a front desk to answer the phone, and the phone is where the money
arrives — new patients, reschedules, cancellations that free sellable slots. rinity today is
a receptionist's *tool*, not a receptionist: one working scheduling screen behind an internal
allowlist door, no calls, no voice, no signup, and a calendar that fails a human's hands
(EV-020, EV-026). audgent can already run the call (EV-011) but no seam exists to rinity
(EV-010 §3). The bet: make rinity the thing the practice buys to **answer the phone itself**
(EV-013) — self-serve (EV-016), audgent invisible inside it (EV-015), sold per office at the
high end (EV-024), with RCDA as the first real practice on it (EV-025).

## Bound

**None on scope** (owner: EV-014) — comprehensiveness decides which competitors we can
tackle. The bounding mechanism is the **tier ladder** (EV-021): each rung clears one thing
the product could be and leaves a lower rung that can be sold. This pitch is **tier 1 only**;
dental-only (multi-vertical deferred, EV-017). *Proposed* review-session budget
(`bounds-not-time` — no time estimates anywhere): ① pitch freeze, ② design gate
(`system/design-gate.md`), ③ data gate (`system/data-gate.md`), ④ working-software
inspection on synthetic calls (T9 boundary), ⑤ RCDA cutover acceptance. Each ends
continue / cut-scope / kill. Standing offer: pin ⑤ to a real RCDA date.

## Tier 1 (the cut — DEBATE-001 J-4)

Inbound only. The agent must:

1. **Answer and act** — book, reschedule, cancel end-to-end; identify the caller; land the
   outcome on the calendar (F-1, F-2).
2. **Answer the practice's questions** from configured facts (hours, parking, payment,
   accepted plans); anything deeper is captured verbatim with a promised follow-up — never a
   stonewall, never live eligibility (COL-3 resolution).
3. **Recognize urgency** and route per office policy (A2-6; F-5).
4. **Hand off to a human** when asked or stuck, with honest fallback when nobody answers (F-5).

The practice must be able to:

5. **Review every call** — recording, verbatim transcript, actions taken, including failed
   and partial calls; the review queue is the console's center of gravity (F-4, D1).
6. **Correct the agent's work** on a schedule desk that meets the human bar (EV-026: New
   Visit works, `t` moves the calendar, drag-and-drop confirms — F-6).
7. **Configure and test the agent** — practice facts, policies, voice; browser test call
   before any phone line and before billing (F-7, C2-4).
8. **Arrive self-serve** — rinity.com → signup → configured console; locked (gated)
   integrations visible but nonblocking (F-8, EV-022).
9. **Connect one line** via one productized path with instant rollback (F-9, A2-8).

And we must be able to **operate the fleet** (F-10).

## Solution shape

1. **The seam (DEBATE-001 J-1)** — a rinity-authored, engine-held **office ledger**: rinity
   distributes availability projections, agent config, and engine artifacts ahead of calls;
   the engine commits bookings locally mid-call with **no cross-service round-trip**; every
   commit reconciles through rinity's pms-adapter boundary after the call; **rinity alone
   holds PMS credentials**; reconcile conflicts surface as explicit F-4 queue items.
   audgent remains pure-internal and contract-frozen at its engine boundary (EV-008).
2. **Call records (J-2)** — every answered call projects into rinity as a first-class
   customer record: recording, verbatim transcript, actions, outcome, links to affected
   appointments; negative cases included. audgent artifacts are projected, not linked-to;
   practices never see audgent (D1-3, EV-015).
3. **The door (J-3, decision candidate)** — dedicated branded commercial identity realm
   (id.rinity.com per identity-realms doctrine), reached from rinity.com with zero identity
   vocabulary; no product-local auth; internal staff via restricted federation; per-office
   pricing stated on the front door before identity capture (C2-5, EV-024).
4. **Data posture (J-2)** — **no pre-compliance real-call phase.** Real patient calls begin
   only inside the compliant posture (EV-018's trigger is RCDA, EV-025); until then all
   calls are synthetic (T9). Pre-cutover packet work, named: artifact-token
   expiry/revocation (D2-5), QA/webhooks/integrations default-off (D2-6), fixture endpoints
   unreachable from live workflows (D2-3), Postgres classified PHI-bearing (D2-4), vendor
   egress allowlist + BAAs (D2-8). Retention numbers land at the data gate (D1-4).
5. **Schedule desk** — rebuilt or repaired to the EV-026 human bar; it is the correction
   surface the sale depends on, not a side feature.
6. **Dogfood** — RCDA runs on the product (EV-025). Sequencing of its go-live
   prerequisites is **E-1** (below).

## Flows (summary — full addendum in `flows/FLOWS-001-receptionist-replacement.md`)

| ID | Flow | Actor | Surface |
| --- | --- | --- | --- |
| F-1 | Agent answers and books | PATIENT ↔ AGENT | phone |
| F-2 | Reschedule / cancel by phone | PATIENT ↔ AGENT | phone |
| F-3 | Practice questions answered or captured | PATIENT ↔ AGENT | phone |
| F-4 | **Desk reviews the agent's work — the center of gravity** | DESK | web |
| F-5 | Urgency + human handoff | PATIENT → DESK | phone→web |
| F-6 | Schedule desk (EV-026 human bar) | DESK | web |
| F-7 | Configure and test the agent (browser test call) | OWNER | web |
| F-8 | Self-serve signup → configured console | OWNER | web |
| F-9 | Connect a line, instant rollback | OWNER | web |
| F-10 | Fleet operations | OPERATOR | web |

Not flows: outbound anything (EV-023), patient web surface, usage-metered billing.

## Acceptance (the stranger-practice loop — BRIEF-001 bar)

A stranger practice finds rinity.com → signs up self-serve and lands in a configured
console without learning one internal word → hears the agent as their practice in a browser
test call → connects their line → a real caller books, reschedules, and asks about
insurance, and every outcome lands on the calendar → the desk reviews the calls (recording,
transcript, actions), corrects one appointment on a desk that behaves under human hands →
and pays the per-office price. In parallel: RCDA's real line runs on it (subject to E-1).

## Rabbit holes (named, avoided)

Live insurance eligibility (gated integration, later rung); bespoke PMS work per prospect
(EV-022 — gated behind sales calls); perfect voice naturalness (parity on the normal
Tuesday, not a Turing test); multi-location orgs beyond one office = one billing atom
(EV-024); rebuilding audgent internals (engine contract is frozen — config/artifacts only);
synchronous booking APIs in the call path (COL-1 — rejected, dead air).

## No-gos

Outbound calls (recall, confirmation, waitlist — EV-023, next rung); any patient-facing web
surface; multi-vertical (EV-017); usage-metered pricing (EV-024); audgent visible to any
customer in any pixel, URL, or email (EV-015); real patient audio before the compliant
posture exists (J-2).

## Escalation carried to owner (E-1)

May RCDA go live behind an interim internal door and/or staged realm/compliance evidence
(friendly pilot, EV-005), or are the branded id.rinity.com realm and full compliance
posture hard gates even for RCDA? This pitch assumes the **conservative reading** (hard
gates); a relax ruling moves RCDA cutover earlier without changing tier-1 scope.

## On freeze

Tag `pitch/rinity-1`. Work packets derive per `system/work-packets.md` (including
peripheral coverage: actor journeys, empty/error/edge states, UX artifacts) and live in the
rinity repo. The data-gate item list is already staged in FLOWS-001. Deltas arrive as PRs
against this pitch, never edits.
