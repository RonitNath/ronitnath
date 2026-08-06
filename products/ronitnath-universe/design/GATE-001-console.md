---
gate: design (session ② of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-05
status: AWAITING OWNER REVIEW
artifact: Penpot "Design gate — universe pitch-1 (console)" — file 310e8e79-e53d-81b8-8008-707f2932c1b6, team ronitnath.com
generators: universe-ronitnath `design/penpot/` (boards are regenerable; scripts are the source)
---

# Design gate — console

Scope is the **console only**. Presence keeps the night-sky language; invite pages are bespoke per
invite (EV-014) and therefore have no canonical layout to lock — that is why the console overview is
the base screen (owner ruling). Console language is hallmark `ember` (DEC-004).

## What is on the canvas

**One layer per Penpot page** (owner correction 2026-08-05 — the layers were first built on a single
page, which is wrong: each layer is its own review surface).

| Page | Boards | What the owner checks |
| --- | --- | --- |
| `1 · Base screen` | Console event overview, ember dark + light, 1440×1024 | Direction, dark/light, typography, spacing, density |
| `2 · Component gallery` | Every component, both themes | The vibe the application gives off |
| `3 · User flows` | 5 rows, 23 screens: sign-in; agent-creates-owner-refines; mint & share invite; guest RSVP; express past events | **Completeness and ordering of the workflows** — the gate's stated purpose |
| `4 · Breakpoints` | Phone 390 / tablet 834 / desktop 1440 | Phone-first behaviour: rail → tabs, pinned primary action |
| `5 · Options` | Ember vs brass vs ember-light, same screen | The identity decision |

All colours are **linked library assets** (`ember/dark/*`, `ember/light/*`, `brass/dark/*`) — editing
one asset updates every board. Values transcribed exactly from the hallmark contract. Prototypes are
wired per page (9 named starting points: one each on pages 1, 2, 4, 5 and five on page 3), so every
layer clicks through in Play mode.

## Decisions this gate needs

1. **Identity** — ember (taxonomy match: consoles are internal ops surfaces) or brass (carries
   ronitnath.com's gold heritage). Built for real as layer 5 rather than argued in prose.
2. **Workflow completeness and ordering** — strike, add, or reorder anything in layer 3. Changes land
   as packet deltas in `changes/pitch-1/packets.md`.
3. **Design-language sign-off** — after which the tokens are the contract the build styles against.

## Deliberately not here

Invite-page layouts (bespoke per invite); presence surfaces (night-sky brief, unchanged); the photos
area, calendar and circles (later bets); anything the data gate may surface — new UI discovered there
loops back to layers 3–4.

## Known gaps in the artifact

- Layer 3 screens are **wireframe fidelity**, not styled comps: they exist to check that the journeys
  are complete and correctly ordered, which is what the owner asked the gate to do. Styling is judged
  on layers 1, 2 and 5.
- The empty/error/expired states enumerated in the packet coverage checklist appear inside the flow
  rows (wrong password, session expired, conflict, dead link, unmappable import) rather than as a
  separate state matrix.
