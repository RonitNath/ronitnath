---
gate: design (session ② of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-05
status: DECISIONS TAKEN 2026-08-05 (ember; base screen = people directory; flows signed off) — layer 1/3 rebuild outstanding
artifact: Penpot "Design gate — universe pitch-1 (console)" — file 310e8e79-e53d-81b8-8008-707f2932c1b6, team ronitnath.com
generators: universe-ronitnath `design/penpot/` (boards are regenerable; scripts are the source)
---

# Design gate — console

## Outcome of the first review (2026-08-05)

The owner reviewed the five pages and rejected the flow layer, on grounds that reached past the
drawing into the product and the process:

1. **The flows were the wrong flows.** Import isn't a user journey at all (EV-017), and the editing
   journey was wrong: copy is a TOML file the owner edits directly, structure goes through the coding
   agent (EV-015). The admin panel is per-event too (EV-016), which knocks out the base-screen ruling
   this gate was built on.
2. **The flows were drawn wrong.** Thumbnail-sized boards strung in rows; they must be **full pages
   at real viewport size, one flow per Penpot page**.
3. **The process was wrong.** Discovering all of the above *at the gate* means flows belong at the
   pitch — a separate addendum, summarized in the pitch, in scope for debate. Now SYS-DEC-004.
4. **A canvas bug**: near-white section headings over Penpot's light canvas, and drawn labels
   colliding with Penpot's own board-name labels. Canvas text must contrast with the *canvas*.
5. **Options pages are conditional** — they exist only when there's a live decision to put in front
   of the owner, not as a standing layer. (The identity pick is a live decision, so page 5 stands.)

Recorded as EV-015/016/017, DEC-005, PITCH-001 delta-3, packets delta-3.

## Current state of the file

| Page | State |
| --- | --- |
| `1 · Base screen` | **Rebuild against the people directory** (F-9/PKT-12). The ember styling, typography and density hold; the screen it renders does not. |
| `2 · Component gallery` | **Stands, and is promoted** — with both admin panels and invite pages composed per instance, the library *is* the design artifact (DEC-005). Add PKT-14's copy-button states and the disconnected-stream indicator. |
| `3 · User flows` | **Withdrawn; rebuild from FLOWS-001** — ten pages, one per flow, full viewport, with before/after pairs on the streaming surfaces. |
| `4 · Breakpoints` | Stands; phone-first behaviour unaffected. |
| `5 · Options` | **Retires** — ember is picked, so there is no live decision for it to hold. |

Colours remain **linked library assets** (`ember/dark/*`, `ember/light/*`, `brass/dark/*`), values
transcribed from the hallmark contract, so a token change still cascades everywhere.

## Owner decisions taken (2026-08-05)

1. **Identity: `ember`.** Picked from the option boards; DEC-004 resolved. Page 5 has done its job and
   retires — options pages exist only while a decision is live.
2. **Base screen: the people directory** (F-9, PKT-12). Layer 1 rebuilds against it; the per-event
   admin overview it currently renders is a bespoke composition and can't be canonical.
3. **Flows signed off** — with F-3, F-5 and F-9 rewritten and F-10 added in the same pass
   (delta-4). Layer 3 rebuilds from FLOWS-001: **ten pages, one per flow**, full viewport.
4. **F-3's seam resolved out of the design gate**: copy is database rows edited in the console, not a
   file (EV-018). It stopped being a design question and became a scope decision.

Still outstanding: **design-language sign-off** on the rebuilt layer 1, after which the tokens are the
contract the build styles against.

## What the rebuild has to show that the first one didn't

SSE (DEC-006) makes three surfaces *change while being looked at* — F-3 copy landing on open pages,
F-6 counts moving as guests answer, F-9 the directory filling in. Static boards can't show that, so
each gets a **before/after board pair** plus its **disconnected** state, which is a first-class state
now: numbers that stopped updating while still presenting as live are the failure this must design
against. PKT-14's copy-button states (never / once / many) go in the gallery — that affordance is
what makes a long invitee list workable.

## Deliberately not here

Invite-page layouts (bespoke per invite); presence surfaces (night-sky brief, unchanged); photos,
calendar, circles (later bets); import (no UI at all — EV-017).
