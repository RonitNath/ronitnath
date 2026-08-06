---
id: EV-028
date: 2026-08-06
provenance: ground-truth
source: owner, DEBATE-001 review (portfolio session 2026-08-06)
---
"Rinity can also be pulling data or receiving pushes from the pms, so we have close to real
time updates on the schedulable windows."

The office ledger's availability projection (J-1) is kept fresh by **polling the PMS and/or
receiving PMS pushes** — near-real-time schedulable windows, not a stale snapshot distributed
once before calls. Freshness contract and adapter capability (which PMSes push vs must be
polled) are data-gate items.
