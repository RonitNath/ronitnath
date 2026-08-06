---
id: FLOWS-001
product: ronitnath-universe
pitch: PITCH-001 (addendum, added retroactively by delta-3 — SYS-DEC-004)
date: 2026-08-05
status: SIGNED OFF 2026-08-05 (owner: "Lgtm on the rest of the flows") — F-3, F-5, F-9 rewritten and F-10 added by the same review; those four await confirmation
sources: EV-012, EV-015, EV-016, EV-017, EV-018, EV-019, EV-020, DEC-005, DEC-006
---

# User flows — event platform + minimum identity

Ten flows. Written per `system/flow-addendum.md`: journeys that happen in a terminal are flows too,
and one-time machine-mediated work is not.

**Actors**: OWNER (browser session), GUEST (holds a capability link, no account), VISITOR (public
web, no link), AGENT (machine, owner-minted token — no UI, so no flows of its own; it appears as a
mediator inside owner flows). FRIEND (account holder) is a named non-goal: zero flows, deliberate.

**Surface legend**: `web` = the platform UI · `terminal` = work with the AI coding agent ·
`mixed` = crosses the boundary mid-journey.

**Updates stream.** SSE is the default transport for anything that changes while someone is looking
at it (DEC-006): RSVPs landing on the admin dashboard, copy edits reaching live pages, the people
directory as identities appear. Polling and manual refresh are the exception and must be argued for.

| ID | Flow | Actor | Surface | Canonical or per-event |
| --- | --- | --- | --- | --- |
| F-1 | Sign in | OWNER | web | canonical |
| F-2 | Create an event by talking to the agent | OWNER | mixed | canonical tail |
| F-3 | Edit an event's copy | OWNER | web | canonical page, per-event content |
| F-4 | Change structure | OWNER | terminal | per-event code |
| F-5 | Invite people and share links | OWNER | web | canonical |
| F-6 | Watch responses come in live | OWNER | web | **per-event composition** |
| F-7 | Open an invite and respond | GUEST | web | per-event composition |
| F-8 | Arrive without a link | VISITOR | web | canonical |
| F-9 | People directory | OWNER | web | canonical — **base screen** |
| F-10 | Manage an identity | OWNER | web | canonical |

---

## F-1 Sign in

- **actor**: OWNER · **surface**: web · **packets**: PKT-01
- **trigger**: Owner opens an admin URL, or returns after a session lapsed.
- **outcome**: Owner holds a session cookie; admin surfaces are reachable.
- **steps**:
  1. Owner hits any admin URL → redirected to sign-in, target URL retained.
  2. Enters email + password → session cookie set (token stored only as a hash).
  3. Lands on the target URL, or the **people directory** (F-9) if there wasn't one.
- **branches**: already signed in → straight through, no sign-in screen.
- **states**: wrong password / unknown email → one generic failure (no user enumeration); session
  expired mid-form → return to sign-in and *keep the target*; no-DB or migration-pending → the app
  refuses to boot rather than half-serving.
- **open**: none.

## F-2 Create an event by talking to the agent

- **actor**: OWNER (mediated by AGENT) · **surface**: mixed · **packets**: PKT-03, PKT-04, PKT-12
- **trigger**: Owner wants an event to exist — "pickleball on August 15th."
- **outcome**: The event exists, unpublished, with its own page, its own admin panel and its own
  visual identity; its copy is seeded in the database; the owner has seen it and published it.
- **steps**:
  1. *(terminal)* Owner describes the event to the coding agent: what it is, when, who, what matters.
  2. *(terminal)* Agent authors the event — page composition, admin-panel composition, any
     event-specific components, and the visual identity **as code**.
  3. *(terminal)* Agent creates the runtime record over the HTTP/MCP API with its named token,
     **including the event's initial copy rows**; audit rows carry the token label.
  4. *(web)* The event appears in the event index, marked unpublished.
  5. *(web)* Owner opens the preview — the real page, exactly as a guest would see it.
  6. *(web)* Owner publishes. Visitors can reach it; the invitee list opens for business (F-5).
- **branches**: owner doesn't like it → back to step 1, no UI path; publish later → the event sits
  unpublished and invites may still be minted (warned).
- **states**: event created but its code not yet deployed (see `open`); index empty (first-run);
  preview of an event whose copy rows are incomplete.
- **open**: **the deploy seam.** Steps 2 and 3 split across two systems — code that must ship and rows
  that land over an API. If the event's code isn't live when the record appears, the index shows an
  event that can't render. Data gate settles the ordering and what the index shows in between.

## F-3 Edit an event's copy

- **actor**: OWNER · **surface**: web · **packets**: PKT-11, PKT-13
- **trigger**: A typo, a time change, a sentence that reads wrong — the most frequent change there is.
- **outcome**: The database holds the new wording and **every page currently open is already showing
  it** — no redeploy, no refresh.
- **steps**:
  1. Owner opens the event → **Copy** page in the console.
  2. Fields are grouped and ordered the way the page reads, so editing feels like reading the page.
  3. Owner edits a value and saves.
  4. The row updates; the change is pushed over SSE to **every active page for that event** —
     guest invite pages included — and re-renders in place.
  5. Owner watches it change in the preview beside the form.
- **branches**: several fields in one pass; an agent writing copy over the API at the same time
  (F-2 step 3) lands on the same rows and streams out identically.
- **states**: a value that fails its field's type/length rule → rejected at save with the field
  named, never half-applied; a guest **mid-RSVP** when copy changes → their form state survives the
  re-render (the copy updates around them); a viewer offline or reconnecting → gets current copy on
  reconnect, not a gap; concurrent owner+agent write on one field (last-write-wins, surfaced).
- **open**: does the SSE frame carry the changed field or the whole copy set for the event? Does copy
  keep history — is there an undo, or is the previous value gone? Data gate.

## F-4 Change structure

- **actor**: OWNER (mediated by AGENT) · **surface**: terminal · **packets**: PKT-03, PKT-04
- **trigger**: The event needs something it doesn't have — a schedule tracker, a different order, a
  component that doesn't exist yet.
- **outcome**: The event's page or admin panel is different; other events are untouched.
- **steps**:
  1. *(terminal)* Owner tells the agent what should change.
  2. *(terminal)* Agent edits that event's composition, or writes a new component and registers it.
  3. *(terminal)* Change ships.
  4. *(web)* Owner confirms on the live page.
- **branches**: the change generalizes → agent promotes the component into the shared library for
  future events (this is how the library grows — DEC-005 consequence 2).
- **states**: a component removed while copy rows still reference it; an archived event keeps
  rendering in its frozen form regardless (DEBATE-001 C-7).
- **open**: does promotion into the shared library need an owner decision, or is it the agent's call?

## F-5 Invite people and share links

- **actor**: OWNER · **surface**: web · **packets**: PKT-02, PKT-06, PKT-14
- **trigger**: The event is real and people should know about it.
- **outcome**: Everyone on the invitee list has a working link, and the owner can see at a glance
  who he has actually sent to.
- **steps**:
  1. Owner opens the event's **invitees**.
  2. Adds people from the people directory (F-9), searching existing contacts, or creates a contact
     inline — name plus optional nickname.
  3. **Links mint automatically**, one per invitee, no per-link configuration step. The URL is the
     person's **name-slug plus a short hash**; the *page* greets them by **nickname** (EV-019).
  4. Owner works down the list clicking **copy**, and sends each link himself — in a text, a DM,
     wherever that person actually is. Sharing happens outside the product, mechanically.
  5. **Each copy button remembers**: it changes appearance once copied and carries a count of how
     many times. Going down a long list, or coming back to it out of order, the owner can see who
     he has already handled and who is still untouched. This is the flow's defining affordance —
     without it the list is unreadable at the size that matters.
  6. The list is **dynamic**: adding someone later mints their link immediately, no batch step.
- **branches**: **re-mint** → a new hash, the old URL dies on next request (the July 4th invalidation
  story); remove someone from the list; one **shareable** link with no person attached, for a group
  chat, at the public tier.
- **states**: two invitees with the same name (the hash disambiguates, the slug stays readable); a
  contact with no nickname (falls back to their name); **never-copied entries as the event
  approaches** — the thing the owner most wants to see; clipboard write fails (the URL must still be
  selectable); a revoked link that was already sent.
- **non-goals in this bet**: per-invite custom pages and per-invite composition (EV-020 — deferred,
  supersedes EV-014); any personalization at sharing time.
- **open**: is the copy count persisted server-side (surviving reload and a different device) or
  local to the browser? Server-side is the recommendation — the owner works a long list across
  sittings and a laptop-vs-phone split would silently lose his place. Data gate.

## F-6 Watch responses come in live

- **actor**: OWNER · **surface**: web, **per-event composition** · **packets**: PKT-05, PKT-07, PKT-13
- **trigger**: Before and during the event — July 4th precedent: this is the page held in one hand at
  the party.
- **outcome**: Owner knows who's coming, who hasn't answered, and whatever else *this* event needs,
  without ever pulling to refresh.
- **steps**:
  1. Owner opens the event's admin panel — composed for this event from library components plus any
     event-specific ones (EV-016).
  2. Counts: going / maybe / can't / no reply, party total.
  3. Guest list with notes and per-segment answers where the event uses segments.
  4. **A guest answers and the panel updates in place, over SSE** — the count moves, their row
     changes, no refresh (EV-018). Same for a changed answer.
  5. Owner acts on it — chases a no-reply back through F-5.
- **branches**: phone in hand at the event (the primary rendering, not the variant); an event with a
  bespoke tile — B24's schedule tracker, July 4th's segment counts — that no other event has.
- **states**: no guests yet; everyone answered; **stream disconnected** — the panel must say so rather
  than quietly showing stale numbers, and must resync on reconnect without a full reload; a burst of
  answers at once; concurrent agent write.
- **open**: how much of this panel is library components versus event-specific code, in practice —
  the first real answer to DEC-005's revisit condition.

## F-7 Open an invite and respond

- **actor**: GUEST · **surface**: web · **packets**: PKT-02, PKT-03, PKT-07, PKT-13
- **trigger**: Guest taps the link the owner sent, usually on a phone, usually once.
- **outcome**: The owner knows whether they're coming; the guest needed no account.
- **steps**:
  1. Guest opens the link → the event page renders at the link's tier, greeting them **by nickname**.
  2. Guest reads the event as the owner composed it for this event.
  3. Guest sets status, party size, and a note; per-segment answers if the event uses segments.
  4. Confirmation is on the page itself — the answer is now visible as *their* answer, and it has
     already reached the owner's dashboard (F-6).
- **branches**: **returning guest** — same link, sees their current answer, changes it, upserts
  against the same identity row (no duplicate people); no-JS → the form posts and works, without the
  live updates.
- **states**: revoked / expired / malformed token → a friendly dead-link page, a 404-equivalent that
  reveals nothing about whether the event exists; link for an unpublished event; revoked mid-visit;
  double-submit; **copy changing under them while they read** (F-3 step 4) without disturbing what
  they've typed.
- **non-goals in this bet**: per-invite page composition (EV-020); guest login (FRIEND, later bet).
- **open**: none.

## F-8 Arrive without a link

- **actor**: VISITOR · **surface**: web · **packets**: PKT-02, PKT-09
- **trigger**: Someone types ronitnath.com, or a published event URL is passed around.
- **outcome**: They see what's public and nothing else.
- **steps**:
  1. Visitor lands on presence (night-sky language, unchanged — `docs/design.md`).
  2. A published event reached without a link renders at public tier: private fields absent, not
     hidden-but-present.
  3. No RSVP path without a link.
- **branches**: unpublished event URL → indistinguishable from a nonexistent one.
- **states**: tier filter lives in exactly one place (PKT-02 constraint), so this flow is largely a
  *check* on F-7's rendering rather than its own set of screens.
- **open**: does an anonymous visitor hold an SSE connection for copy updates, or is live update an
  authenticated-or-tokened privilege? Cheapest correct answer is probably yes-for-everyone, but it
  is a fan-out question. Data gate.

## F-9 People directory  *(base screen)*

- **actor**: OWNER · **surface**: web, canonical · **packets**: PKT-12, PKT-08
- **trigger**: Planning an event, deciding who to invite, or just looking someone up.
- **outcome**: Owner sees everyone in the system, one row per human, with their history.
- **steps**:
  1. Owner opens **People** — the console's home, and the platform's densest canonical surface.
  2. One row per person across all events: name, nickname, events invited to, events attended, last
     response. **Not** one row per event-attendance — the collapse is the point (EV-013).
  3. Search and filter; sort by how recently they've been around.
  4. Open a person → every event they were invited to, what they answered, whether they showed, and
     their contact record.
  5. From here, into F-5 (invite them to something) or F-10 (fix their record).
- **branches**: **the directory is live** — a new RSVP or a newly created identity appears without a
  refresh (EV-018). When friends can eventually sign in and maintain their own records, those edits
  stream in here too; that is a later bet, and the seam is being built now so it doesn't need
  retrofitting.
- **states**: empty (first-run, before import); legacy duplicates awaiting merge (PKT-08 collapses
  them deliberately); a person with no events; a person whose only record is a never-opened link.
- **open**: which fields live on the identity versus the contact record — data gate.

## F-10 Manage an identity

- **actor**: OWNER · **surface**: web, canonical · **packets**: PKT-12
- **trigger**: A new person to add, a nickname to set, two rows that are the same human, or someone
  who should not be in the system.
- **outcome**: The directory tells the truth about who these people are.
- **steps**:
  1. **Create** — add a person by name; optionally a nickname and contact details. This is also
     reachable inline from F-5 step 2, so inviting someone new never requires leaving the flow.
  2. **Read** — the person's full record and cross-event history (F-9 step 4).
  3. **Update** — edit name, nickname, contact details. The nickname is what events greet them by;
     the name is what their link slug is built from (EV-019), so changing the name raises the
     question of whether existing links follow.
  4. **Delete** — remove a person from the system.
- **branches**: **merge two identities** into one, which is how import duplicates get resolved — the
  survivor keeps every attendance record from both.
- **states**: merging two identities that both responded to the *same* event with *different*
  answers (the conflict needs an explicit rule, not a silent pick); deleting a person who has
  attendance history — the owner's stated reason for this whole bet is that this data must not be
  lost, so delete is almost certainly archive; creating a person whose name matches an existing one
  (allowed — the hash disambiguates links — but flagged so duplicates aren't made by accident).
- **open**: merge conflict rule; delete vs archive semantics; whether renaming a person re-slugs
  their live links or leaves them. All three are data-gate questions and all three are
  irreversibility-flavoured (T3).

---

## Not flows (deliberate)

- **Expressing the three past events** — one-time, agent-mediated, no owner journey and no screens
  (EV-017). Stays as PKT-08 with an import-notes file; still acceptance-relevant.
- **Friend signup / accounts** — pitch no-go; schema-ready only. The live-update seam built for F-6
  and F-9 is what their edits will eventually stream through.
- **Photos, calendar, circles, notifications** — separate bets.
- **Agent journeys** — AGENT has no UI. Its work appears as steps inside F-2 and F-4; its contract
  is PKT-04.

## Design-gate rendering

One Penpot page per flow, boards at real viewport size, in journey order. F-4 gets a page despite
being `terminal`: it shows the state before and after the conversation, which is where the deploy
seam becomes visible. Live-updating surfaces (F-3, F-6, F-9) need a before/after board pair so the
streamed change is legible as motion, not just a final state.
