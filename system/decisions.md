# System-level decisions (the operating system itself)

## SYS-DEC-001 — Bounds, not time appetites (2026-08-05)

Chose **review-session budgets + optional real-world deadlines** over Shape Up's week-denominated appetite, because owner ground truth from months of agent-driven work: time-amounts are almost never accurate or useful when agents do the building. What appetite exists to do — fix the budget before elaboration so scope flexes instead of growing — is preserved; only the denomination changes to the actually-scarce resources:

- **Review-session budget**: N owner inspection sessions for the bet. Every session ends with an explicit continue / cut-scope / kill. Budget exhausted = bet stops and is re-pitched, same as an appetite expiring.
- **Real-world deadline** (when one exists): a date reality enforces — an event, a demo, a commitment. Strictly better than either time or session counts; prefer it when available.

Estimates of duration are never recorded anywhere in this system.

## SYS-DEC-002 — Peripheral work is a pipeline station, not a prompt (2026-08-05)

Owner flagged at the pilot bet's first pitch freeze: user stories and adjacent "peripheral" work (edge states, UX flows, verification evidence) had no home, and the owner couldn't name how to ask for it. Fix: the **work-packet layer** (`system/work-packets.md`) is a mandatory station between pitch freeze and build, with a coverage checklist (actors×journeys, states, data lifecycle, instrumentation, UX artifacts, verification) run mechanically at derivation. The owner should never need to remember a category of work; the checklist remembers. Waivers are written, never silent.

## SYS-DEC-003 — Design gate and data gate before build (2026-08-05)

Owner added two review stations between packet derivation and build, because inspecting working software is too late to catch a missing workflow, a wrong design direction, or a wrong domain model:

- **Design gate** (`design-gate.md`): layered Penpot review — dense base screen with linked tokens → all user flows → select flows at breakpoints, all prototyped (component-gallery layer struck by owner 2026-08-06, rinity EV-043); option pages for iteration-by-selection. Owner verifies workflow completeness + ordering and locks the design language.
- **Data gate** (`data-gate.md`): domain model, access patterns, transport seams (HTTP/SSE/WS/RPC), operational posture (elastic scaling, zero-downtime rolling upgrades), peripheral-service requirements. May loop back to the design gate when iteration reveals new UI surfaces.

Both are owner sessions and count against the bet's bound.

## SYS-DEC-004 — User flows are a pitch addendum, not a design-gate discovery (2026-08-05)

Owner ruling at the pilot bet's first design gate, from the gate itself failing in a specific way: reviewing
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

## SYS-DEC-005 — The debate panel is derived per bet, never inherited (2026-08-06)

Chose **teaching panel construction** over shipping a standing set of lenses, on the owner's ruling
that "the appropriate heuristics to use are dependent on the nature of the project, including the
number of agents to use."

The first panel was built for one bet and worked — eight lenses on four axes, catching five defects in
artifacts the orchestrating agent had written itself. Writing those eight into the system as *the*
panel would have carried the wrong thing forward: the axes were derived from that product's specific
tensions, and on a different product most of them have nothing at stake. Two lenses with nothing at
stake agree, and agreement on an untouched axis reads as corroboration — a false signal, worse than an
absent one.

So `debate-heuristics.md` now carries the method — how to find axes from the decisions in play, the
mandate/success/**forbidden-move** lens format, and the rule that agent count is the axis count
doubled rather than a budget picked in advance. What generalised was the *structure* of disagreement,
not its content.

Recorded in the same breath, because it is what makes the method executable: **lenses run as separate
OS processes, not as sub-agents of the agent holding the artifact** (`debate-runner.md`). Sub-agents
inherit the context being decorrelated, and any briefing the parent writes is the author's account of
the thing under attack. The orchestrator writes the panel, launches, and fans in — it does not explain
the artifact to anyone.

## SYS-DEC-006 — Unspecified stations are marked, not implied (2026-08-06)

The loop names more stations than the system has specs for; the later ones (working-software
inspection, acceptance, retro) have never run. Chose to **publish a maturity column in the README**
naming each station specified / thin / unspecified, rather than either writing speculative specs or
leaving the gap silent.

Speculative specs are the worse failure: a station spec written before the station has failed encodes
guesses with the same authority as the rules that were paid for, and an agent cannot tell which is
which. Every spec in `system/` was written or rewritten after its station failed in a nameable way,
and that provenance is the reason to trust it.

An unspecified station is **unwritten, not optional** — it still runs, by judgment, and the spec gets
written from what goes wrong.
