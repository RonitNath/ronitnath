---
product: rinity
pitch: PITCH-001 — replace the receptionist (tier 1)
date: 2026-08-06
status: FROZEN 2026-08-06 (owner: "lgtm, go" + hallmark-brass amendment EV-039; bound = 5 review sessions proposed and unobjected, session ① spent at freeze)
sources: BRIEF-001, FLOWS-001 (rev 2), DEBATE-001, DEC-001, DEC-002
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
inspection on asterisk-driven synthetic calls, ⑤ RCDA cutover acceptance — reached via the
staged ladder (DEC-001): asterisk → owner-phone real calls → RCDA fake-patient phase →
compliant posture → real patients. Each session ends continue / cut-scope / kill. Standing
offer: pin ⑤ to a real RCDA date.

## Tier 1 (the cut — DEBATE-001 J-4 + owner batch EV-031..EV-038)

Inbound only. The agent must:

1. **Answer and act** — book, reschedule, cancel end-to-end; identify the caller; land the
   outcome on the calendar (F-1, F-2). Offered slots are **held** the moment they're
   offered and resolve to bookings when the caller has said enough (DEC-002).
2. **Offer slots strategically** — ~2 per week then advance, so callers can't enumerate the
   schedule and pickiness costs the caller, not the practice; per-office customizable
   (EV-033).
3. **Answer the practice's questions** from configured facts (hours, parking, payment,
   accepted plans); anything deeper is captured verbatim with a promised follow-up — never a
   stonewall, never live eligibility (COL-3 resolution).
4. **Recognize urgency** and route per office policy (A2-6; F-5).
5. **Hand off to a human** when asked or stuck, with honest fallback when nobody answers (F-5).
6. **Run in the office's chosen answering model** — full answering, overflow (agent works
   only after the office doesn't pick up), or dedicated agent-bookable PMS slots, where
   office-open ≠ agent-bookable (EV-032).

The practice must be able to:

7. **Review every call** — recording, verbatim transcript, actions taken, **and an LLM
   grade** (per-aspect agent-behavior ratings, caller-emotion analysis, resolution
   efficacy/speed — EV-034), including failed and partial calls; the review queue is the
   console's center of gravity (F-4, D1).
8. **Listen in live** on calls in progress (F-11, EV-037).
9. **See what they're paying for** — call volume and quality analytics on the console
   (F-12, EV-036).
10. **Correct the agent's work** on a schedule desk that meets the human bar (EV-026: New
    Visit works, `t` moves the calendar, drag-and-drop confirms — F-6).
11. **Configure and test the agent** — practice facts, policies, answering model, offer
    policy, voice; browser test call before any phone line and before billing (F-7, C2-4).
12. **Arrive self-serve through spec'd onboarding** — rinity.com → signup → **give the
    practice's website URL; rinity scrapes everything it can and asks only for what it
    couldn't figure out** (EV-031) → configured console; locked (gated) integrations
    visible but nonblocking (F-8, EV-022). Bar: RCDA has seen good AI product before
    (ai-assistant) — onboarding must meet that standard.
13. **Connect one line** via one productized path with instant rollback (F-9, A2-8).

And we must be able to **operate the fleet** (F-10) — including **cost analytics: cost per
call, component-level drivers, per-office projections** (EV-035), with **every test run
saved, labeled, and drill-downable in the same cost ledger** (EV-038; run labeling may be
an audgent contract change).

## Solution shape

1. **The seam (DEBATE-001 J-1, refined by DEC-002)** — a rinity-authored, engine-held
   **office ledger** with a **hold-then-resolve protocol**: rinity keeps the availability
   projection near-real-time by polling the PMS and/or receiving PMS pushes (EV-028) and
   distributes it with agent config and engine artifacts ahead of calls; the agent **holds**
   slots as it offers them and resolves holds to bookings mid-call with **no cross-service
   round-trip**; every commit reconciles through rinity's pms-adapter boundary; **rinity
   alone holds PMS credentials**; the rare conflict (a hold beaten by an out-of-band PMS
   write) surfaces as an explicit F-4 queue item. audgent remains pure-internal and
   contract-frozen at its engine boundary (EV-008).
2. **Call records (J-2)** — every answered call projects into rinity as a first-class
   customer record: recording, verbatim transcript, actions, outcome, **LLM grade**
   (EV-034), links to affected appointments; negative cases included. audgent artifacts are
   projected, not linked-to; practices never see audgent (D1-3, EV-015).
3. **The door (J-3, decision candidate)** — dedicated branded commercial identity realm
   (id.rinity.com per identity-realms doctrine), reached from rinity.com with zero identity
   vocabulary; no product-local auth; internal staff via restricted federation; per-office
   pricing stated on the front door before identity capture (C2-5, EV-024).
4. **Data posture (J-2, boundary revised by DEC-001/EV-029)** — **no real patient data
   pre-compliance.** The go-live ladder: asterisk-driven testing (already integrated with
   audgent) carries most volume → the owner personally drives real PSTN calls from their
   own number → RCDA staff knowingly discuss **fake patients** on real calls (also proving
   the no-PHI operating mode audgent's future non-healthcare businesses will use) → the
   compliant posture arrives (EV-018) → real patients. Pre-real-patient packet work, named:
   artifact-token expiry/revocation (D2-5), QA/webhooks/integrations default-off (D2-6),
   fixture endpoints unreachable from live workflows (D2-3), Postgres classified
   PHI-bearing (D2-4), vendor egress allowlist + BAAs (D2-8). Retention numbers land at the
   data gate (D1-4).
5. **Schedule desk** — rebuilt or repaired to the EV-026 human bar; it is the correction
   surface the sale depends on, not a side feature. Console identity is **hallmark brass**
   (gold-external) — the current ember (red-internal) is a defect to correct (EV-039).
6. **Analytics spine** — one cost/quality data path feeds three surfaces: per-call grades
   (EV-034) → practice analytics (volume + quality, F-12) and the operator cost surface
   (cost per call, drivers, per-office projections, F-10/EV-035); test traffic lives in the
   same ledger, labeled (EV-038).
7. **Dogfood** — RCDA runs on the product (EV-025), entering via the staged ladder behind
   the interim internal door (DEC-001); the branded realm gates the first stranger
   practice, not RCDA.

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
| F-9 | Connect a line (answering model choice), instant rollback | OWNER | web |
| F-10 | Fleet operations + cost analytics + testing ledger | OPERATOR | web |
| F-11 | Listen in live on a call in progress | DESK/OWNER/ISO | web |
| F-12 | Practice analytics: volume + quality | OWNER | web |

Not flows: outbound anything (EV-023), patient web surface, usage-metered billing.

## Acceptance (the stranger-practice loop — BRIEF-001 bar)

A stranger practice finds rinity.com → signs up, gives their website, and lands in a
console already configured with what rinity figured out, answering only the gaps → hears
the agent as their practice in a browser test call → connects their line in their chosen
answering model → a real caller books, reschedules, and asks about insurance; offered slots
are held as offered, and every outcome lands on the calendar → the desk reviews the calls
(recording, transcript, actions, grade), listens in on one live, corrects one appointment
on a desk that behaves under human hands → the owner reads their volume/quality analytics →
and pays the per-office price. In parallel: RCDA's real line runs on it via the DEC-001
ladder, and ISO can say what any call cost and why.

## Rabbit holes (named, avoided)

Live insurance eligibility (gated integration, later rung); bespoke PMS work per prospect
(EV-022 — gated behind sales calls); perfect voice naturalness (parity on the normal
Tuesday, not a Turing test); multi-location orgs beyond one office = one billing atom
(EV-024); rebuilding audgent internals (engine contract may gain narrow additions — run
labeling EV-038, live tap EV-037 — but stays config/artifact-driven); synchronous booking
APIs in the call path (COL-1 — rejected, dead air); perfect website scraping (the
ask-only-gaps design *is* the bound — scrape what's scrapable, interview the rest,
EV-031); grading-model sophistication (a useful rubric beats a research project, EV-034).

## No-gos

Outbound calls (recall, confirmation, waitlist — EV-023, next rung); any patient-facing web
surface; multi-vertical (EV-017); usage-metered pricing (EV-024); audgent visible to any
customer in any pixel, URL, or email (EV-015); real patient **data** before the compliant
posture exists (J-2 as revised by DEC-001 — real calls about fake patients are the staging
mechanism, not the exception).

## Debate record

DEBATE-001 (T3+T2): 3 collisions resolved (seam COL-1, data posture COL-2, insurance depth
COL-3), C axis convergent (branded realm = decision candidate J-3). Escalation E-1 ruled
same day: interim door yes → DEC-001 (staged go-live ladder); J-1's residual double-booking
risk closed by DEC-002 (hold-then-resolve + near-real-time PMS sync).

## On freeze

Tag `pitch/rinity-1`. Work packets derive per `system/work-packets.md` (including
peripheral coverage: actor journeys, empty/error/edge states, UX artifacts) and live in the
rinity repo. The data-gate item list is already staged in FLOWS-001. Deltas arrive as PRs
against this pitch, never edits.
