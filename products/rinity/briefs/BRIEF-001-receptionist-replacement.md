---
product: rinity
bet: receptionist replacement (first rinity bet under portfolio management)
date: 2026-08-06
interviewer: claude (portfolio session)
status: final
confidence: medium-high   # every topic covered; two topics answered by interviewer judgment as directed
---

# BRIEF-001 — replace the receptionist

## Agenda coverage

| Topic | Status | Distillation |
| --- | --- | --- |
| Problem | covered (delegated to dogfood, EV-012) | Product is a receptionist's tool, not a receptionist — voice has zero surface (EV-020) |
| The bar | covered (delegated, "up to you") | Derived below: the six-step stranger-practice loop |
| Users | covered | "all" — practice owner, front-desk staff, callers/patients, Isoastra operator |
| Scope | covered | "replace the receptionist" (EV-013) |
| Bound | covered | none — comprehensiveness picks the competitor tier (EV-014) |
| Constraints | covered | "any" — nothing frozen beyond audgent-as-engine (EV-008, EV-009) |
| No-gos | covered | audgent internal / rinity external (EV-015); self-serve required (EV-016); multi-vertical deferred (EV-017); compliance is a runway (EV-018) |
| Success signals | covered | cash flow (EV-019) |
| Risks | covered ("what you'd expect") | patient data, live-line cutover, agent-says-wrong-thing; feeds debate triggers |

## Answers

**Problem.** Rinity's namesake capability is absent from the product. The console is one
excellent screen — a live, adapter-backed schedule desk with real booking, editing, provider
load, SSE updates (EV-020) — and nothing else: no voice surface, no second screen, no
self-serve, no front door. The engine that answers calls exists and is staged (EV-011:
7-provider telephony, full STT/LLM/TTS pipeline, T9 harness, capacity numbers) but the two
planes are unwired (EV-010 §3) and audgent must stay invisible to customers (EV-015). The
problem this bet solves: **turn a schedule desk plus a headless engine into a product a
practice can buy, trust, and pay for.**

**The bar** (derived per delegation; makes EV-004 concrete, consistent with EV-019). Rinity
clears the founder's bar when a **stranger practice** — not RCDA, no Isoastra human in the
loop — can complete this loop:

1. Find the product and understand it (front door)
2. Sign up and reach their own console (self-serve, EV-016 — org/office of their own, not an allowlist)
3. Connect a phone line and their system of record (or start on rinity-native scheduling)
4. Configure their agent — greeting, hours, booking policy, escalation, the owner's business philosophy (EV-002)
5. The agent answers a real call and completes a receptionist task end-to-end; the result lands on the calendar
6. The practice reviews what the agent did (calls, transcripts, outcomes) and pays (EV-019)

Steps survive without hand-holding ⇒ deployable; a stranger completing them ⇒ marketable;
recurring payment ⇒ won. Today step 5's "lands on the calendar" target exists — the other
five steps are unbuilt.

**Users.** All four. Whose behavior changes most: the *caller* stops reaching a human; the
*front desk* stops answering the phone and starts reviewing the agent's work — the console's
center of gravity shifts from "book visits" to "supervise the receptionist."

**Scope.** The receptionist's duty surface is the requirements source (EV-013): answer,
book/reschedule/cancel (exists as human-operated UI), answer practice questions (hours,
insurance, directions), take messages, new-patient intake, confirmations/recall (outbound —
see OQ-1). Comprehensiveness across that surface is the scope dial (EV-014).

**Bound.** None (EV-014, recorded waiver of the session-bound norm). The flow addendum and
pitch should express scope as competitive tiers: which coverage level beats an answering
service, which beats the incumbent dental voice-AI products, which beats a human receptionist.

**Constraints.** Audgent is the engine (EV-008) and is pure-internal (EV-015); everything
else revisitable (EV-009). Standing architecture (adapter contract, Kanidm door, tenancy
spine — EV-010) survives on merit, not by grandfathering.

**No-gos.** Multi-vertical (EV-017). Compliance posture is deferred until the first real
business, but no compliance-hostile decisions in the meantime (EV-018).

**Risks / irreversibility** (feeds debate triggers): patient-data handling once real calls
flow (PHI in transcripts/recordings — retention and residency decisions are hard to walk
back); cutting over a live practice line (RCDA's 925 line, EV-005); the agent saying wrong
things to real patients (trust, and possibly liability, is spent faster than it is earned);
per-practice agent-behavior model (config schema becomes a compatibility surface).

## Open questions

| # | Question | Why it matters | Blocking? |
| --- | --- | --- | --- |
| OQ-1 | Is outbound work (confirmations, recall, callbacks) in the receptionist surface for this bet, or inbound-only first? | Sizes the engine surface and the console's task model | no |
| OQ-2 | Where does self-serve end? PMS connection needs adapter credentials a practice can't mint alone — is "self-serve to demo, assisted to PMS" acceptable? | Shapes onboarding flows and the adapter story | no |
| OQ-3 | Pricing shape (per line / per call / per office flat)? | Cash flow (EV-019) needs a billing surface; shape changes the console | no |
| OQ-4 | Is the RCDA line cutover part of this bet or a separate deploy decision? | EV-005 posture; first real traffic vs stranger-practice readiness | no |
| OQ-5 | Which competitor tier is the first target (answering service / dental voice-AI incumbents / human parity)? | EV-014 makes this the de-facto scope decision | no |

## Next stations

Flow addendum (enumerate the human journeys: caller, front desk, owner, Isoastra operator —
per SYS-DEC-004 they are debated with the pitch); debate-trigger evaluation (T3
irreversibility fires on PHI + live-line; the two-plane seam decision likely warrants a
panel); then PITCH-001.
