---
id: DEC-002
date: 2026-08-06
source: owner refinement of DEBATE-001 J-1 (EV-027, EV-028)
---
Chose **hold-then-resolve reservations over optimistic local commits**, closing J-1's named
residual risk (the commit→reconcile double-booking window) with a protocol instead of an
accepted exposure.

**Holds.** When the agent starts negotiating times, it requests **holds** on candidate slots
— the slot is reserved early, while the caller is still supplying information. A hold
resolves to a booking once identity and visit details are sufficient, or expires (TTL, a
data-gate number). Two concurrent callers can no longer be promised the same slot: the
second caller never has it offered.

**Freshness.** The office ledger's availability projection is not a pre-call snapshot: rinity
**polls the PMS and/or receives PMS pushes** for near-real-time schedulable windows (EV-028).
Which adapters push vs poll, and the staleness bound the agent may act on, are data-gate
items.

**What J-1 keeps:** the engine still commits against the engine-held ledger with no mid-call
cross-service round-trip; rinity still reconciles through the pms-adapter boundary and
remains sole PMS-credential holder; conflicts (now rare by construction — a hold beaten by
an out-of-band PMS write) still surface as explicit F-4 queue items.

**Interaction with dedicated-slots mode (EV-032):** when the office runs agent-reserved
custom slots, the hold pool is that slot set, making collisions with human PMS writes
structurally impossible — the modes compose.
