---
id: EV-039
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-9)
---
"kanidm"

Resolves OQ-9. Authentication for voice.ronitnath.com goes through Kanidm, matching the
house pattern used by devmail and the internal services rather than the customer-facing
Rauthy realm. Consequence: the service has a real trust boundary from the start — one human
principal, the owner — so `procedures/security.md` applies and `~/dev/context/isoastra/
agent-identity.md` governs client lifecycle. Cheap to do at rung 1 and expensive to retrofit
under EV-026's adversarial work later. Note it does not conflict with EV-013's ugly-and-
internal licence: unauthenticated is not what internal-only means.
