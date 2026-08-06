# Spec: the user-flow addendum (pitch stage)

User flows are written **at the pitch stage**, as a separate document beside the pitch, frozen with
it, and **in scope for debate**. They are not discovered at the design gate. (SYS-DEC-004 — the gate
that produced this rule.)

Why the pitch and not the gate: a flow is a claim about how a human reaches an outcome. Wrong flows
are as expensive as a wrong schema and cheaper to fix in prose than in boards — and if the owner is
still *arguing about which journeys exist* while looking at a Penpot canvas, the canvas was drawn
against guesses. The design gate's job is to render agreed flows and check ordering, fidelity and
feel; it is not where the flow set is invented.

## Where it lives

```
products/<name>/flows/FLOWS-###-<slug>.md     # the addendum — full detail
products/<name>/pitches/PITCH-###-<slug>.md   # carries a one-line summary per flow + a link
```

The pitch stays at problem altitude: it lists flow IDs, actors and one-line outcomes. The addendum
holds the steps. Both freeze under the same tag; both change only by delta.

## Per-flow format

```
F-##  <name>
actor:      one actor from the pitch
trigger:    what starts it (a real-world event, not a click)
outcome:    the state of the world when it's done
surface:    web | outside-web | machine-only
steps:      numbered, each naming the surface it happens on
branches:   the alternate paths a real person takes
states:     empty / error / expired / conflict, at the step where each can occur
packets:    PKT-## IDs derived from this flow
open:       questions this flow raises for the data gate
```

## Rules

1. **A journey that happens outside the web UI is still a flow.** Editing a file, talking to an
   agent, running a command, a phone call — write it, mark `surface: outside-web`. Omitting
   non-UI journeys is how an incomplete product looks complete: the screens all exist and the work
   between them is invisible.
2. **One-time and machine-mediated work is not a flow.** A migration an agent runs once is a work
   packet, not a journey. Writing it as a flow invents UI that will never be built.
3. **An actor with zero flows is an error** — either they have a journey or they are a named non-goal.
4. **Flows feed packets, not the reverse.** The packet layer derives from the frozen flow set; the
   coverage checklist (`work-packets.md`) then checks the derivation, rather than being the place
   journeys are first enumerated.
5. **Flows are debated when the pitch is debated.** The debate's argument graph may carry claims
   about the flow set; a struck or reordered flow lands as a delta before freeze.

## At the design gate

Each agreed flow gets **its own Penpot page**, rendered at real viewport size, in journey order.
`surface: outside-web` flows get a page too — showing what the owner sees before and after the
non-UI step, which is exactly where a missing seam shows up.
