---
gate: design (session ② of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-05
status: PARTIALLY REJECTED — flow layer withdrawn, rebuild blocked on FLOWS-001 sign-off
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
| `1 · Base screen` | **Stands as a styling check, void as a structural one** — it renders a per-event admin overview, which DEC-005 says has no canonical layout. Direction/typography/density feedback still counts. Base screen moves to the **people view** (PKT-12) on rebuild. |
| `2 · Component gallery` | **Stands, and is promoted** — with both admin panels and invite pages composed per instance, the library *is* the design artifact (DEC-005). |
| `3 · User flows` | **Withdrawn.** 23 thumbnail screens against a superseded flow set. Rebuild is one page per flow, full viewport, from FLOWS-001 — after sign-off, not before. |
| `4 · Breakpoints` | Stands; phone-first behaviour unaffected by the pivot. |
| `5 · Options` | Stands — ember vs brass is a live decision. |

Colours remain **linked library assets** (`ember/dark/*`, `ember/light/*`, `brass/dark/*`), values
transcribed from the hallmark contract, so a token change still cascades everywhere.

## What the owner decides next

1. **FLOWS-001 sign-off** — strike, add, reorder the nine flows. This is now a document review, not a
   canvas review. Rebuild of layer 3 waits on it.
2. **Identity** — ember or brass (page 5, built for real).
3. **Base screen** — confirm the people view as the canonical dense screen, or name another.
4. **Design-language sign-off** — after which the tokens are the contract the build styles against.

## Open question this gate surfaced for the data gate

F-3's delivery seam: how an edited `copy.toml` reaches production — compiled in, read at runtime, or
DB-stored and edited through a plain TOML box. PKT-11 cannot be built until it is settled, and the
answer decides whether F-3 has a UI at all.

## Deliberately not here

Invite-page layouts (bespoke per invite); presence surfaces (night-sky brief, unchanged); photos,
calendar, circles (later bets); import (no UI at all — EV-017).
