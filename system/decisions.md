# System-level decisions (the operating system itself)

## SYS-DEC-001 — Bounds, not time appetites (2026-08-05)

Chose **review-session budgets + optional real-world deadlines** over Shape Up's week-denominated appetite, because owner ground truth from months of agent-driven work: time-amounts are almost never accurate or useful when agents do the building. What appetite exists to do — fix the budget before elaboration so scope flexes instead of growing — is preserved; only the denomination changes to the actually-scarce resources:

- **Review-session budget**: N owner inspection sessions for the bet. Every session ends with an explicit continue / cut-scope / kill. Budget exhausted = bet stops and is re-pitched, same as an appetite expiring.
- **Real-world deadline** (when one exists): a date reality enforces — an event, a demo, a commitment. Strictly better than either time or session counts; prefer it when available.

Estimates of duration are never recorded anywhere in this system.

## SYS-DEC-002 — Peripheral work is a pipeline station, not a prompt (2026-08-05)

Owner flagged at PITCH-001 freeze: user stories and adjacent "peripheral" work (edge states, UX flows, verification evidence) had no home, and the owner couldn't name how to ask for it. Fix: the **work-packet layer** (`system/work-packets.md`) is a mandatory station between pitch freeze and build, with a coverage checklist (actors×journeys, states, data lifecycle, instrumentation, UX artifacts, verification) run mechanically at derivation. The owner should never need to remember a category of work; the checklist remembers. Waivers are written, never silent.
