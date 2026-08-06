---
gate: design (session ② of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-05
status: REDESIGNED 2026-08-06 — 16 pages, 69 boards, one prototype flow per page. Second owner review taken; awaiting design-language sign-off on layer 1, the last gate item.
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

**16 pages · 69 boards · one named prototype flow per page**, boards chained in journey order so the
owner clicks through rather than reading stills. Redesigned 2026-08-06 against the five language
rules above; the coverage below is unchanged from the rebuild.

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

## Outcome of the second review (2026-08-06)

The owner reviewed the rebuild and rejected **how it reads**, not what it covers. The coverage
rulings from the first review all stood; the design language did not.

1. **The type hierarchy was noise.** Nineteen sizes across the file, with the size ladder doing
   work that colour and space should do. Collapsed to five: `28 / 21 / 17 / 15 / 12`.
2. **Titles were heavier, not just larger.** Ruled: *"at most a little larger, and not heavier in
   weight."* Weight 600 is now absent from the system — 400 is everything, 500 lifts an active
   control or a column header, and a title is a title because of its size and the display face.
3. **Eyebrows are out, wholesale.** Named specifically: the `Hey Shaz` greeting on the invite,
   which "reads as an eyebrow". Generalised — field labels, section kickers and count captions all
   stacked small-over-large. `stat()` now puts the number first with its label underneath.
4. **The explanatory quips are the deep one.** Eight strings named by hand — *"Ronit already
   knows"*, *"Saves carry the value's revision…"*, *"Confirm the yeses…"*, *"updating as people
   answer"*, *"• Live"*, *"event-specific — no other event has this"*, and others — and identified
   as **"a broader pattern of AI-built interface preference I greatly dislike."** The rule taken
   from it: a board states what the owner's data is; it never justifies its own design, reassures,
   or narrates. Every such line is deleted or moved to the canvas, which is where gate notes belong.
5. **Route chips are out.** The `/accounts`, `/e/…` chip in each board's top right is gone. Routes
   live in DASH-001 and the flow addendum.

Two consequences of rule 4 that look like omissions and are not. **The healthy stream state now
renders nothing** — a standing green LIVE badge tells the owner only what he already assumes, and
the whole of DEC-006 is that he finds out when it *stops* being true. And **status is coloured text
rather than a chip**; a badge survives only in the events index, where the lifecycle state is the
row's subject rather than an annotation on it.

### Two product decisions came out of this review

- **The nickname is removed from the bet.** The owner's account: it was closer to a plugin, wanted
  situationally and per event, not a directory column. `contact` keeps phone and email; pages greet
  people by name. Recorded as an EV; it also removes the F-5 "greeted as" column and the
  identity-vs-contact demo field that made EV-021's seam visible, which the two panels still carry.
- **Answering-as was built on a premise that cannot hold.** The owner's question — *"how would the
  link know if it's been forwarded?"* — has no answer. A bearer link cannot detect forwarding, so
  the "You're answering as Nikhil" banner was a warning fired by a detection that does not exist.
  The fix is the owner's own: the headline is **`{person_name}'s invite to {event_title}`**,
  rendered unconditionally. Whoever opens it reads whose answer they are about to change, and the
  system never claims to know how the link travelled. The board survives as a *forwarded* case that
  renders identically — only the name differs, because it is Nikhil's link.

**F-13 rebuilt to spec** (2 boards → 4): active sessions rather than a "signed in" timestamp,
password reset inline on the row, and the account row is a route. `/accounts/{id}` carries every
live session with device, coarse location and last-seen, three revocation granularities
(per-session, selected, all-others), and that account's audit log. The current session is marked
and has no revoke control.

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
