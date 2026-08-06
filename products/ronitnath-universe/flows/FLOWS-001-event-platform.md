---
id: FLOWS-001
product: ronitnath-universe
pitch: PITCH-001 (addendum, added retroactively by delta-3 — SYS-DEC-004)
date: 2026-08-05
status: DRAFT — awaiting owner strike/reorder, then re-freeze with the pitch
sources: EV-012, EV-014, EV-015, EV-016, EV-017, DEC-005
---

# User flows — event platform + minimum identity

Nine flows. Written per `system/flow-addendum.md`: journeys that happen outside the web UI are
flows too, and one-time machine-mediated work is not.

**Actors**: OWNER (browser session), GUEST (holds a capability link, no account), VISITOR (public
web, no link), AGENT (machine, owner-minted token — no UI, so no flows of its own; it appears as a
mediator inside owner flows). FRIEND (account holder) is a named non-goal: zero flows, deliberate.

**Surface legend**: `web` = happens in the platform UI · `outside-web` = happens in a text editor, a
terminal, or a conversation with the coding agent · `mixed` = crosses the boundary mid-journey.

| ID | Flow | Actor | Surface | Canonical or per-event |
| --- | --- | --- | --- | --- |
| F-1 | Sign in | OWNER | web | canonical |
| F-2 | Create an event by talking to the agent | OWNER | mixed | canonical tail |
| F-3 | Change wording | OWNER | mixed | per-event file |
| F-4 | Change structure | OWNER | outside-web | per-event code |
| F-5 | Mint and share invites | OWNER | web | canonical |
| F-6 | Watch responses | OWNER | web | **per-event composition** |
| F-7 | Open an invite and respond | GUEST | web | **per-invite composition** |
| F-8 | Arrive without a link | VISITOR | web | canonical |
| F-9 | Look up a person across events | OWNER | web | canonical |

---

## F-1 Sign in

- **actor**: OWNER · **surface**: web · **packets**: PKT-01
- **trigger**: Owner opens an admin URL, or returns after a session lapsed.
- **outcome**: Owner holds a session cookie; admin surfaces are reachable.
- **steps**:
  1. Owner hits any admin URL → redirected to sign-in, target URL retained.
  2. Enters email + password → session cookie set (token stored only as a hash).
  3. Lands on the target URL, or the event index if there wasn't one.
- **branches**: already signed in → straight through, no sign-in screen.
- **states**: wrong password / unknown email → one generic failure (no user enumeration); session
  expired mid-form → return to sign-in and *keep the target*; no-DB or migration-pending → the app
  refuses to boot rather than half-serving.
- **open**: none.

## F-2 Create an event by talking to the agent

- **actor**: OWNER (mediated by AGENT) · **surface**: mixed · **packets**: PKT-03, PKT-04, PKT-05
- **trigger**: Owner wants an event to exist — "pickleball on August 15th."
- **outcome**: The event exists, unpublished, with its own page, its own admin panel, its own copy
  file and its own visual identity; the owner has seen it and published it.
- **steps**:
  1. *(outside-web)* Owner describes the event to the coding agent: what it is, when, who, what
     matters about it.
  2. *(outside-web)* Agent authors the event — page composition, admin-panel composition, any
     event-specific components, the visual identity **as code**, and a `copy.toml` holding wording.
  3. *(outside-web)* Agent creates the runtime record over the HTTP/MCP API with its named token;
     audit rows carry the token label.
  4. *(web)* The event appears in the **event index**, marked unpublished.
  5. *(web)* Owner opens the event preview — the real page, exactly as a guest would see it.
  6. *(web)* Owner publishes. Visitors can now reach it; invites can be minted (F-5).
- **branches**: owner doesn't like it → back to step 1 with the agent, no UI path; publish later →
  event sits unpublished indefinitely and invites may still be minted (warned).
- **states**: event created but deploy not yet carrying its code (see `open`); index empty
  (first-run); preview of an event whose copy file is missing or malformed.
- **open**: **the deploy seam.** Steps 2 and 3 split across two systems — code that must ship and a
  record that lands over an API. If the event's code isn't live when the record appears, the index
  shows an event that can't render. Data gate must settle the ordering and what the index shows in
  between.

## F-3 Change wording

- **actor**: OWNER · **surface**: mixed · **packets**: PKT-11 (new)
- **trigger**: A typo, a time change, a sentence that reads wrong — the most frequent change there is.
- **outcome**: The live page shows the new wording.
- **steps**:
  1. *(outside-web)* Owner opens the event's `copy.toml` and edits it directly. Keys are readable and
     grouped the way the page reads.
  2. *(???)* The edit reaches production.
  3. *(web)* Owner loads the event page and sees the change.
- **branches**: none — this is deliberately the shortest journey in the product.
- **states**: malformed TOML → the page must not break; the last good copy serves and the error is
  visible to the owner somewhere he'll actually look. Key present in the file but not used by any
  module, and module expecting a key the file doesn't have — both need a defined answer.
- **open**: **step 2 is unresolved and it is the load-bearing question of this flow.** Three shapes:
  (a) compiled in — every typo is a rebuild and redeploy; (b) read from disk at runtime — edit on the
  server, or ship the file alone; (c) stored in the DB, edited through a plain TOML text box in the
  console — still "just editing a toml", but the edit-to-live path is one action and works from a
  phone during an event. (c) is the recommendation into the data gate; the owner's phrasing
  ("going through a toml file... directly") is satisfied by any of the three.

## F-4 Change structure

- **actor**: OWNER (mediated by AGENT) · **surface**: outside-web · **packets**: PKT-03, PKT-04
- **trigger**: The event needs something it doesn't have — a schedule tracker, a different order, a
  component that doesn't exist yet.
- **outcome**: The event's page or admin panel is different; other events are untouched.
- **steps**:
  1. *(outside-web)* Owner tells the agent what should change.
  2. *(outside-web)* Agent edits that event's composition, or writes a new component and registers it.
  3. *(outside-web)* Change ships.
  4. *(web)* Owner confirms on the live page.
- **branches**: the change generalizes → agent promotes the component into the shared library, and
  it becomes available to future events (this is how the library grows — DEC-005 consequence 2).
- **states**: a component removed while an invite still references it (PKT-06 state); an archived
  event must keep rendering in its frozen form regardless (DEBATE-001 C-7).
- **open**: does promotion into the shared library need an owner decision, or is it the agent's call?

## F-5 Mint and share invites

- **actor**: OWNER · **surface**: web · **packets**: PKT-02, PKT-06
- **trigger**: Owner is ready to invite people, one at a time or in a batch.
- **outcome**: Each person has a link that shows them what the owner wants them to see.
- **steps**:
  1. Owner opens the event's links surface.
  2. Mints a link: names the person (personalized) or leaves it shareable; picks a tier
     (public / private fields).
  3. **Sets what this invite shows** — module ordering and selection, defaulting to the event's
     composition when untouched (delta-2, EV-014).
  4. Copies the full URL and sends it however he's sending it (outside the product).
  5. The link table shows uses, last-used, and never-used at a glance.
- **branches**: revoke → the link dies on next request, and re-minting for the same person revokes
  the prior one (the July 4th invalidation story); mint for an unpublished event → allowed, warned.
- **states**: revoking an already-revoked link; invite composition referencing a module later removed
  from the event; a link that has never been opened as the event approaches (the thing the owner
  actually wants to see).
- **open**: does per-invite composition live on the capability link, an invite-group, or a
  page-variant entity? (delta-2's open question — data gate.)

## F-6 Watch responses

- **actor**: OWNER · **surface**: web, **per-event composition** · **packets**: PKT-05, PKT-07
- **trigger**: Before and during the event — July 4th precedent: this is the page held in one hand at
  the party.
- **outcome**: Owner knows who's coming, who hasn't answered, and whatever else *this* event needs.
- **steps**:
  1. Owner opens the event's admin panel — composed for this event from library components plus any
     event-specific ones (EV-016).
  2. Reads the counts: going / maybe / can't / no reply, party size total.
  3. Scans the guest list with notes and per-segment answers where the event uses segments.
  4. Acts on it — chases a no-reply by minting or re-sending a link (→ F-5).
- **branches**: phone in hand at the event (the primary rendering, not the variant); an event with a
  bespoke tile — B24's schedule tracker, July 4th's segment counts — that no other event has.
- **states**: no guests yet; everyone answered; a guest who answered then went silent; concurrent
  agent write (last-write-wins, surfaced as "changed since you loaded").
- **open**: how much of this panel is library components versus event-specific code, in practice —
  the first real answer to DEC-005's revisit condition.

## F-7 Open an invite and respond

- **actor**: GUEST · **surface**: web, **per-invite composition** · **packets**: PKT-02, PKT-03, PKT-07
- **trigger**: Guest taps the link the owner sent, usually on a phone, usually once.
- **outcome**: The owner knows whether they're coming; the guest needed no account.
- **steps**:
  1. Guest opens the link → the event page renders at the link's tier, in this invite's composition,
     greeting them by name if the link is personalized.
  2. Guest reads what the owner put first for *them* (EV-014 — the ordering is the message).
  3. Guest sets status, party size, and a note; per-segment answers if the event uses segments.
  4. Confirmation is on the page itself — the answer is now visible as *their* answer.
- **branches**: **returning guest** — same link, sees their current answer, changes it, and it
  upserts against the same identity row (no duplicate people); no-JS → the form posts and works.
- **states**: revoked / expired / malformed token → a friendly dead-link page that is a 404-equivalent
  and reveals nothing about whether the event exists; link for an unpublished event; revoked
  mid-visit; double-submit.
- **open**: none.

## F-8 Arrive without a link

- **actor**: VISITOR · **surface**: web · **packets**: PKT-02, PKT-09
- **trigger**: Someone types ronitnath.com, or a published event URL is passed around.
- **outcome**: They see what's public and nothing else.
- **steps**:
  1. Visitor lands on presence (night-sky language, unchanged — `docs/design.md`).
  2. A published event, reached without a link, renders at public tier: private fields absent, not
     hidden-but-present.
  3. No RSVP path without a link.
- **branches**: unpublished event URL → indistinguishable from a nonexistent one.
- **states**: tier filter must live in exactly one place (PKT-02 constraint), so this flow is largely
  a *check* on F-7's rendering rather than its own set of screens.
- **open**: none.

## F-9 Look up a person across events

- **actor**: OWNER · **surface**: web, canonical · **packets**: PKT-08, PKT-01
- **trigger**: "Has Nikhil come to any of these?" — planning the next event, or before minting invites.
- **outcome**: One row per human, with their history across every event.
- **steps**:
  1. Owner opens the people view.
  2. Searches or scans; one row per person, not one per event-attendance.
  3. Opens a person → every event they were invited to, what they answered, whether they showed.
  4. Uses it to decide who to invite (→ F-5).
- **branches**: legacy duplicates collapsed deliberately at import (PKT-08); a person who exists only
  in one past event.
- **states**: no history (a person minted for the first time); a person whose only record is a
  never-opened link.
- **open**: none. **This is the strongest candidate for the design gate's base screen** — the densest
  canonical surface, and the one whose absence across three separate codebases caused the actual loss
  (EV-013).

---

## Not flows (deliberate)

- **Expressing the three past events** — one-time, agent-mediated, no owner journey and no screens
  (EV-017). Stays as PKT-08 with an import-notes file; still acceptance-relevant.
- **Friend signup / accounts** — pitch no-go; schema-ready only.
- **Photos, calendar, circles, notifications** — separate bets.
- **Agent journeys** — AGENT has no UI. Its work appears as steps inside F-2 and F-4, and its
  contract is PKT-04.

## Design-gate rendering

One Penpot page per flow, boards at real viewport size, in journey order. F-3 and F-4 get pages
despite being `outside-web`: they show the state before the file/conversation and the state after,
which is exactly where the missing deploy seam becomes visible.
