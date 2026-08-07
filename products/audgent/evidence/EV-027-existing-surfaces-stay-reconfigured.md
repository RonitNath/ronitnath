---
id: EV-027
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (flow review)
---
"Right now, the audgent configuration surface is a no-code flow builder and a model configuration surface and a number of other surfaces. No reason these need to go, they just need to be reconfigured for a one-org world."

Resolves BRIEF-001/OQ-5 and bounds the UI work sharply. The inherited console's surfaces —
the no-code flow builder, model configuration, and the rest — **stay**. They are not replaced
by a new surface and not discarded; they are re-cut for a world with one organization
(EV-017). Consequence: the human-surface work divides into (a) reshaping what exists, by
removing tenant scoping and re-pointing it at the whole system, and (b) genuinely new views
that have no inherited equivalent — the agent-activity read (EV-014), the production-vs-test
cost split (EV-013), and wire-level drill-down (EV-026). Only (b) is new construction. This
is also consistent with EV-018: modify in place, never rebuild. A proposal to replace the
console is contradicting an owner ruling.
