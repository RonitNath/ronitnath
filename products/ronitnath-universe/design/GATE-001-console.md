---
gate: design (session ② of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-05
status: REBUILT 2026-08-06 — 16 pages, 67 boards, one prototype flow per page. Awaiting design-language sign-off on layer 1, the last gate item.
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

## Current state of the file — rebuilt 2026-08-06

**16 pages · 67 boards · one named prototype flow per page**, boards chained in journey order so the
owner clicks through rather than reading stills.

| Page | State |
| --- | --- |
| `1 · Base screen` | **Rebuilt** against the **dashboard and Contacts**, ember dark and light. Two surfaces, not one: the dashboard is the root, Contacts is where density is actually under load. |
| `2 · Component gallery` | **Rebuilt and promoted** — with admin panels and invite pages both composed per instance, the library *is* the design artifact (DEC-005). Now carries PKT-14's copy-button states and the three stream states. |
| `3a … 3m` | **Thirteen pages, one per flow**, F-1 … F-13, at real viewport size. Streaming surfaces (F-3, F-6, F-9) carry before/after pairs and a disconnected board. Terminal flows (F-2, F-4) render the terminal as a surface rather than skipping it. |
| `4 · Breakpoints` | **Rebuilt** — dashboard 390/834/1440, Contacts 390/1440, invite 390/834. |
| ~~`5 · Options`~~ | **Retired**, as ruled. Ember is decided, so nothing live is left for it to hold. |

Colours remain **linked library assets** (`ember/dark/*`, `ember/light/*`, plus `presence/dark/*` for
the night-sky boards on 3h), values transcribed from the hallmark contract, so a token change still
cascades everywhere.

**Two of the rebuilt boards answer questions the first file didn't raise.** Layer 4's Contacts-at-390
shows the six-column table ceasing to be a table — and the three facts (invited / answered / showed)
surviving that, rather than collapsing into one "attended" number at exactly the width the screen is
read on most. Layer 4's dashboard-at-1440 deliberately does *not* spread to fill the width; the empty
right-hand column is the DASH-001 constraint made visible.

## Owner decisions taken (2026-08-05)

1. **Identity: `ember`.** Picked from the option boards; DEC-004 resolved. Page 5 has done its job and
   retires — options pages exist only while a decision is live.
2. **Base screen: the people directory** (F-9, PKT-12). Layer 1 rebuilds against it; the per-event
   admin overview it currently renders is a bespoke composition and can't be canonical.
3. **Flows signed off** — with F-3, F-5 and F-9 rewritten and F-10 added in the same pass
   (delta-4), then F-12 and F-13 added by delta-6. Layer 3 rebuilt from FLOWS-001: **thirteen pages,
   one per flow**, full viewport.
4. **F-3's seam resolved out of the design gate**: copy is database rows edited in the console, not a
   file (EV-018). It stopped being a design question and became a scope decision.
5. **Root is the personal dashboard, and the route surface is settled** (EV-022, DASH-001, DEC-011),
   which is what turned the layer-1 rebuild into two screens rather than one.

Still outstanding: **design-language sign-off** on the rebuilt layer 1, after which the tokens are the
contract the build styles against. That is now the only open item in this gate.

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
