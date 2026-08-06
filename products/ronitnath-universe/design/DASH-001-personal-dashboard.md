---
id: DASH-001
product: ronitnath-universe
date: 2026-08-05
status: OWNER-CUT 2026-08-05 (four cards; present-event keyed on end time) — lands as flow F-12 + PKT-17
sources: EV-009, EV-022, DEC-006, DEBATE-002 (C-1, C-5, H3-operator-under-load-2)
---

# The personal dashboard — root of ronitnath.com for OWNER

**What it is**: a routing and lightweight health surface. The first thing seen, the place everything
is reached from, and the place that says whether anything needs attention. It is explicitly **not** a
control panel — every action it offers is *go there*, not *do it here*. The per-event dashboard (F-6)
is where work happens.

**Why that line matters**: the moment the dashboard can edit things, it starts duplicating per-event
panels, and per-event panels are bespoke (EV-016) — so the duplication would be N-way. Routing plus
health is the only shape that stays small as the platform grows.

## Cards — cut to four by owner ruling (2026-08-05)

The surface is a short card list, phone-first. Everything not listed here is **deliberately absent**,
not deferred-with-a-placeholder.

| Card | Content | Live? |
| --- | --- | --- |
| **Present event** | The event happening now, or the next one to happen, with RSVP counts as plain `x/y/z` (yes / maybe / no). One tap to its panel. | SSE |
| **Contacts** | `X contacts` — a count, routing to the full CRM (F-9). | SSE |
| **Accounts** | `X accounts` — a count, routing to account management. Today that list is one row: the owner. | SSE |
| **System** | One line, green/amber: deploy state and SHA, stream health, last backup, probe status (PKT-10). Amber routes to detail, never a fix-it button. | SSE |

**"Present event" is defined by the event's END time, not its start** (owner ruling). The rule is
`the event with the soonest end_time still in the future` — so an event stays present *through* its
whole run rather than vanishing from the dashboard the moment it begins, which is precisely when it
matters most. One query, no state, no "is it running" flag to keep correct.

Data-model consequence: **events carry an end time, not just a date.** The predecessors did not all
model this. It goes to the data gate.

This card is also what answers DEBATE-002's operator-under-load objection: at 9pm the root of the
site is the event in progress and its counts.

### Deliberately absent

The earlier draft proposed **Needs you**, **Events** and **People pulse**; all three are cut. An
action queue, an event list and an activity feed are each a way for a routing surface to start
accreting product. If a nag turns out to be needed, it earns its way back with a real instance of
having been forgotten.

### Later (slots, not stubs)

**Photos** (EV-010), **calendar**, **circles**, **friend activity**, and **invitations received** once
the owner is not the only account. Named so the structure is designed against them — the card list
takes new cards without redesign — but nothing renders for them until they exist.

The structural bet: role determines the card set. FRIEND's root is the same component with a
different set (EV-022), which is the per-event composition idea applied to roles.

## Rules

1. **Every card is a route.** No card mutates state — with "needs you" cut, there is no exception.
2. **Cards are independently live.** Each subscribes to its own scope over SSE and owns its own
   disconnected state; a dead stream on one card must not blank the others (PKT-13).
3. **Absent, not empty.** A card with nothing to say does not render. A dashboard of empty boxes is
   how a routing surface becomes noise.
4. **Health is a signal, not a console.** Amber routes to detail. Fixing happens elsewhere, usually
   not in the browser at all.
5. **Phone-first**, like every owner surface — this is checked one-handed more often than not.

## What is deliberately not here

Bulk actions; any editing; charts or analytics; a global search (the directory has search, and a root
search bar invites the dashboard to become an app shell); notification settings (C-10 no-go); anything
that duplicates a per-event panel.

## Open

- **Accounts management is a new surface** with no packet yet. Today it lists one row, the owner, and
  it is the seam where friend accounts arrive (pitch no-go for onboarding, but the surface is where
  that later bet lands). Needs a packet before build.
- **Events need an end time** for the present-event rule. Data gate.
