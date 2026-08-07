---
id: EV-035
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-5)
---
"hosted"

Resolves OQ-5. Rung 2 uses hosted STT providers, not local models. Consequence: provider
credentials and real spend are in scope from rung 2 onward, so the cost ledger (EV-027)
starts earning its keep immediately rather than at the end; network latency is part of every
measurement and cannot be separated from model latency without deliberate instrumentation;
and the system cannot run offline. This also sharpens EV-010 — the low-latency/memory/CPU
goal applies to *our* runtime, since the model compute is someone else's. Notably the
opposite of console's local-cascade posture, consistent with EV-011 discarding it.
