# System-level decisions (the operating system itself)

## SYS-DEC-001 — Bounds, not time appetites (2026-08-05)

Chose **review-session budgets + optional real-world deadlines** over Shape Up's week-denominated appetite, because owner ground truth from months of agent-driven work: time-amounts are almost never accurate or useful when agents do the building. What appetite exists to do — fix the budget before elaboration so scope flexes instead of growing — is preserved; only the denomination changes to the actually-scarce resources:

- **Review-session budget**: N owner inspection sessions for the bet. Every session ends with an explicit continue / cut-scope / kill. Budget exhausted = bet stops and is re-pitched, same as an appetite expiring.
- **Real-world deadline** (when one exists): a date reality enforces — an event, a demo, a commitment. Strictly better than either time or session counts; prefer it when available.

Estimates of duration are never recorded anywhere in this system.

## SYS-DEC-002 — Peripheral work is a pipeline station, not a prompt (2026-08-05)

Owner flagged at PITCH-001 freeze: user stories and adjacent "peripheral" work (edge states, UX flows, verification evidence) had no home, and the owner couldn't name how to ask for it. Fix: the **work-packet layer** (`system/work-packets.md`) is a mandatory station between pitch freeze and build, with a coverage checklist (actors×journeys, states, data lifecycle, instrumentation, UX artifacts, verification) run mechanically at derivation. The owner should never need to remember a category of work; the checklist remembers. Waivers are written, never silent.

## SYS-DEC-003 — Design gate and data gate before build (2026-08-05)

Owner added two review stations between packet derivation and build, because inspecting working software is too late to catch a missing workflow, a wrong design direction, or a wrong domain model:

- **Design gate** (`design-gate.md`): layered Penpot review — dense base screen with linked tokens → component gallery → all user flows → select flows at breakpoints, all prototyped; option pages for iteration-by-selection. Owner verifies workflow completeness + ordering and locks the design language.
- **Data gate** (`data-gate.md`): domain model, access patterns, transport seams (HTTP/SSE/WS/RPC), operational posture (elastic scaling, zero-downtime rolling upgrades), peripheral-service requirements. May loop back to the design gate when iteration reveals new UI surfaces.

Both are owner sessions and count against the bet's bound.

## SYS-DEC-004 — User flows are a pitch addendum, not a design-gate discovery (2026-08-05)

Owner ruling at the PITCH-001 design gate, from the gate itself failing in a specific way: reviewing
the flow boards turned into a conversation about *which journeys exist and who mediates them* — the
import flow wasn't a user journey at all, and the editing journey was the wrong one. The owner's
diagnosis: "that I needed to talk about this above with you is an indication that user flows should
have been part of the first gate, as an addendum to the pitch, and the pitch would have summaries."

So: flows are written at the pitch stage as a **separate document** (`flows/FLOWS-###-*.md`), the
pitch carries **one-line summaries** and links, both freeze under the same tag, and the flow set is
**in scope for debate** (`flow-addendum.md`). The design gate renders agreed flows and judges
ordering, fidelity and feel — it no longer discovers them.

Two rules fall out of what was actually wrong, and both are in the spec: a journey that happens
outside the web UI (editing a file, talking to an agent) is still a flow and must be written; and
one-time, machine-mediated work is a work packet, not a flow — drawing it as one invents UI nobody
will build.
