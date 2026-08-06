---
id: EV-032
date: 2026-08-06
provenance: ground-truth
source: owner, requirement 2 (portfolio session 2026-08-06)
---
"two desired booking models are that the agent only works after the office doesn't pick up,
and also that the agent has dedicated bookable slots in the office, which is handleable as a
limited number of custom slots in the PMS (but it means office is open != slots available
for booking)"

Two office-selectable operating models, both tier-1:

- **Overflow mode** — the agent answers only after the office doesn't pick up (forward on
  no-answer). Changes F-9's line semantics: not a full takeover.
- **Dedicated-slots mode** — the agent books only into a limited set of agent-reserved
  custom slots in the PMS. Consequence the ledger must model: **office-open ≠
  agent-bookable** — the availability projection is the agent's slot pool, not the office
  calendar.
