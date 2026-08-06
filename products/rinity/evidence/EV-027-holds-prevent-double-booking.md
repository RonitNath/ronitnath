---
id: EV-027
date: 2026-08-06
provenance: ground-truth
source: owner, DEBATE-001 review (portfolio session 2026-08-06)
---
"Rinity can prevent double bookings by having calls request holds and then resolve the holds
once the caller has provided sufficient information. This way, the call gets reserved early."

Refines DEBATE-001 J-1's seam and answers its named residual risk (the commit→reconcile
double-booking window): the ledger protocol is **hold-then-resolve**. When a call starts
negotiating, the agent requests a hold on candidate slots early; the hold converts to a
booking once the caller has supplied enough (identity, visit type), or expires. The slot is
reserved from the moment it is offered, not from the moment the booking commits.
