---
id: DEBATE-001
product: rinity
date: 2026-08-06
trigger: "T3 (irreversibility: PHI/call-data retention, customer identity model, config-schema and self-serve URL commitments, the rinity↔audgent seam) + T2 (bound waived entirely, EV-014)"
scope: BRIEF-001 + FLOWS-001 (flow set in scope per SYS-DEC-004); brief OQs all pre-resolved by owner (EV-021..EV-025)
status: FAN-IN PENDING — claims land in DEBATE-001-tier1-seam-identity-phi.yaml
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

Claims, collisions (`rebuts` edges), and resolutions live in the YAML beside this memo; the
memo's options/rejected/recommendation sections are generated from that graph after fan-in.
