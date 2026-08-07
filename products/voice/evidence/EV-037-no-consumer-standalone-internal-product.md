---
id: EV-037
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-7)
---
"there is no consumer, this service is standalone internal product"

Resolves OQ-7 and sequences EV-012. Systems plugging in — rinity and others — is the
eventual shape, but there is no consumer during this phase and therefore no integration
contract to design. Consequence: no public API surface, no plug-in seam, no versioning
obligation, no backward-compatibility burden while the internals are being built; the only
caller is the service's own web tier. This removes the strongest remaining temptation toward
speculative abstraction (EV-009) and means the consumer seam is designed at graduation
(EV-007), against a real consumer, not imagined ones.
