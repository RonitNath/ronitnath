---
id: DEBATE-001
product: rinity
date: 2026-08-06
trigger: "T3 (irreversibility: PHI/call-data retention, customer identity model, config-schema and self-serve URL commitments, the rinity↔audgent seam) + T2 (bound waived entirely, EV-014)"
scope: BRIEF-001 + FLOWS-001 (flow set in scope per SYS-DEC-004); brief OQs all pre-resolved by owner (EV-021..EV-025)
status: RESOLVED (judge fan-in complete) — 1 escalation (E-1) awaiting owner ruling; memo + graph ready for owner review
---

# DEBATE-001 — tier-1 cut, the seam, the customer door, the PHI posture

## Round provenance (debate-heuristics rule 5–6, debate-runner step 7)

- **Lenses:** 8, one OS process each, launched detached and concurrent on nexus
  (`~/tmp/rinity-debate/{prompts,out,log}`), pi CLI → `openai-codex/gpt-5.5` — a different
  model family from the judge.
- **Judge:** the Claude portfolio session that also authored BRIEF-001 and FLOWS-001. **The
  judge is compromised** (rule 6): the panel and the fan-in are the author's. Stated so the
  owner reads the memo as an input, not a verdict.
- **Asymmetry, recorded:** attack lenses were not handed the artifacts arguing conclusions.
  Per-lens context below; all lenses share the evidence log (`products/rinity/evidence/`).

## The axes (derived from the decisions this round serves)

| Axis | Decision it serves | Lens ⟂ Lens | Extra context beyond evidence log |
| --- | --- | --- | --- |
| A — tier-1 cut | What is the first sellable rung (EV-021) of "replace the receptionist" (EV-013)? | `a1-first-dollar` (smallest sellable cut) ⟂ `a2-parity-standard` (credible replacement or it isn't the product) | A1: business-direction.md · A2: FLOWS-001 + BRIEF-001 |
| B — the seam | How do control plane and engine wire (EV-010 §3 gap vs EV-011 §2 surface), given audgent is invisible (EV-015)? | `b1-rinity-sovereign` (thick control plane, engine replaceable) ⟂ `b2-engine-capable` (no mid-call dependency on a second service) | B1: rinity repo · B2: audgent repo |
| C — customer door | What identity realm admits self-serve practices (EV-016, EV-022) when today's door is an internal allowlist (EV-010/RF-02)? | `c1-realm-consistent` (sanctioned identity architecture, no product-local forks) ⟂ `c2-frictionless-door` (eleven minutes to a configured console) | C1: identity-realms.md + business-direction.md · C2: FLOWS-001 + BRIEF-001 |
| D — call-data posture | What is stored of calls before the compliance trigger fires (EV-018), given review is the trust surface (F-4) and RCDA is in-bet (EV-025)? | `d1-trust-through-records` (verbatim everything from call one) ⟂ `d2-phi-debt-minimizer` (price every stored utterance's reversal cost) | D1: FLOWS-001 + BRIEF-001 · D2: audgent repo (current hardening/staging posture) |

Axis-derivation notes: repair-vs-rebuild of the schedule desk (EV-026) was considered as a
fifth axis and dropped — it is a packet-/design-gate-stage quality question, not a pitch
decision; EV-026 already sets the bar. No T4 evidence contradictions existed after EV-026
superseded EV-020's quality reading.

## Fan-in

Claims, collisions (`rebuts` edges), and resolutions live in the YAML beside this memo
(55 lens claims — every lens staked exactly one claim, all grounded; raw lens outputs
archived in `DEBATE-001-lenses/`). What follows is generated from that graph.

### Collisions found

- **COL-1 (axis B, the real fight): who commits the booking mid-call.** B1-2: every
  scheduling action executes inside rinity's adapter boundary; the engine is just another
  caller of a rinity command contract and never holds PMS credentials. B2-1/B2-6: the
  booking must commit inside the engine runtime with no cross-service round-trip mid-call
  (B2-3 showed the only existing pattern is a 5s-timeout HTTP tool call — dead air waiting
  to happen), backed by an engine-local schedule ledger. Direct, symmetric rebuts.
- **COL-2 (axis D): full fidelity from day one vs minimum PHI before compliance.** D1-1
  gates cutover on every call having durable recording + verbatim transcript + committed
  actions in rinity; D2-1 forbids running any real office on the current audgent posture,
  which durably persists all of that pre-compliance.
- **COL-3 (axis A): insurance depth.** A2-5: callers asking insurance/cost questions must
  get answers from configured facts or precise capture — an accept/don't-accept list is a
  stonewall. A1-3: tier 1 is scheduling-only; eligibility is out.
- **C axis: no collision — convergence.** C1 and C2 both land on a dedicated branded realm.
  Per debate-heuristics, convergence on an axis means the decision was already made
  (here: by the identity-realms doctrine + EV-016 jointly); recorded honestly as a
  decision candidate rather than dressed up as a debate outcome.

### Options considered

1. **Seam — synchronous rinity booking API in the call path** (B1 pure): engine calls
   rinity, rinity calls the adapter, caller waits.
2. **Seam — engine-local commit + reconcile** (synthesis J-1): rinity authors and
   distributes a per-office ledger (availability projection + provisional commit
   authority) to the engine ahead of calls; the engine commits locally mid-call — zero
   cross-service round-trip; every commit reconciles through rinity's PMS-adapter boundary
   post-call; rinity alone holds PMS credentials; reconcile conflicts become explicit F-4
   queue items (FLOWS-001 already specifies conflict states).
3. **Seam — engine talks to the PMS directly** (B2 pure): fastest call path, engine holds
   credentials.
4. **Data posture — learn from real traffic pre-compliance with minimal storage** vs
   **no real calls until the posture exists, then full fidelity** (J-2).
5. **Door — product-local auth** vs **internal Kanidm reuse** vs **dedicated branded
   id.rinity.com realm** (J-3).
6. **Tier-1 cut — scheduling-only** (A1) vs **scheduling + configured-fact answers +
   urgent routing + full review** (J-4).

### Rejected, and why

- **Option 1 (synchronous round-trip):** B2-3's evidence — the current tool-call pattern
  is a 5s-timeout HTTP hop, and a PMS write behind it makes caller-audible dead air a
  structural property. A receptionist replacement that hesitates on "book it" fails A2's
  normal-Tuesday bar and EV-026's human bar.
- **Option 3 (engine holds PMS credentials):** makes the "pure internal" engine (EV-015) a
  PHI+credential blast radius, breaks engine replaceability (B1's mandate), and doubles
  the compliance surface D2 enumerated (D2-4, D2-8).
- **Pre-compliance real traffic:** D2-1's file-level evidence shows the runtime durably
  persists patient audio/transcripts/context today; storing less would gut F-4 (the trust
  surface, D1-1). Both lenses' forbidden moves point the same way: don't run real calls at
  all until the posture exists.
- **Product-local auth:** a forked customer-identity universe to unwind later (C1-2).
  **Kanidm-as-customer-door:** it's a workforce realm with a deploy-time allowlist
  (EV-010/RF-02) — not a signup surface, and doctrine forbids it.
- **Scheduling-only tier 1:** each omission (urgent mishandling A2-6, insurance
  stonewalling A2-5, untrustworthy correction surface A2-7/EV-026) is a named objection
  that loses the high-end per-office sale A1 itself depends on (EV-024).

### Recommendation

- **J-1 (seam):** rinity-authored, engine-held office ledger; engine commits locally
  mid-call, reconciles through rinity's adapter boundary; rinity is sole PMS-credential
  holder; conflicts surface in the F-4 queue. *Knowingly given up:* strict synchronous
  single-writer consistency and engine self-sufficiency. *Residual risk:* the
  double-booking window between local commit and reconcile — accepted, bounded by
  explicit conflict states.
- **J-2 (data posture):** there is no pre-compliance real-call phase. Real calls begin
  only inside the compliant posture (EV-018's trigger is RCDA itself, EV-025); until then
  everything is synthetic under the T9 boundary. Once live, review is full-fidelity and
  rinity-owned; retention numbers are a data-gate item (D1-4). D2's concrete engine gaps
  (non-expiring public artifact tokens D2-5, default-on QA/webhooks D2-6, fixture
  endpoints D2-3, Postgres-as-PHI classification D2-4, vendor egress allowlist D2-8) are
  named pre-cutover packet work.
- **J-3 (door, decision candidate):** dedicated branded id.rinity.com realm per doctrine;
  signup from rinity.com with zero foreign vocabulary; no product-local auth; staff via
  restricted federation; browser test call before billing (C2-4).
- **J-4 (tier-1 cut):** inbound scheduling end-to-end + configured-fact practice answers
  with verbatim capture/promised follow-up + urgent routing per office policy + complete
  call feed including failed/partial calls + one productized line-connection path with
  instant rollback + a correction surface meeting EV-026's human bar. Out: outbound
  (EV-023), live eligibility, bespoke per-prospect PMS work (EV-022), multi-vertical
  (EV-017).

### What would change it

- J-1 reopens if the commit→reconcile double-booking conflict rate is material in
  synthetic load, or if a target PMS's write API makes post-call reconcile unsafe.
- J-2 reopens if the owner rules real-traffic learning is worth pre-compliance retention
  debt (nothing in evidence supports this).
- J-4's insurance line moves when a gated eligibility integration (EV-022) clears its
  sales-call gate.
- J-3 is doctrine-derived; it changes only if the doctrine does.

### Escalation (one, batched — E-1)

**RCDA go-live sequencing.** C1-6 (full realm evidence: branded surfaces, backups,
restore drills, federation audit) and J-2 (compliant posture) jointly gate any real call;
A1-1 presses for first revenue. May RCDA cut over behind an interim internal door and/or
staged realm/compliance evidence given its friendly status (EV-005), or are the branded
realm and full posture hard gates even for RCDA? Blocking (sequences the in-bet cutover,
EV-025) and owner-only (doctrine exception + risk appetite). PITCH-001 is drafted with
the conservative reading (hard gates) and marks where E-1 would relax it.
