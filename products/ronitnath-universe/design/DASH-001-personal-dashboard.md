---
id: DASH-001
product: ronitnath-universe
date: 2026-08-05
status: DRAFT — proposal for owner review, then lands as flow F-12 + PKT-17
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

## Cards

The surface is a **card list**, phone-first, in priority order. Later features add cards; nothing
about the structure changes when they do. Cards render only when they have something to say — no
empty-state boxes for features that don't exist yet.

### Now (built in this bet)

| Card | Content | Live? |
| --- | --- | --- |
| **Happening now** | The event in progress or imminent: name, when, going / no-reply counts, one tap to its panel. Absent when nothing is close. | SSE |
| **Needs you** | The action queue, each row a route: unpublished events, invitees never copied as the date approaches, **events closed but attendance unrecorded** (F-11), copy fields failing validation, **an event whose release isn't live** (PKT-15). | SSE |
| **Events** | Upcoming, then drafts, then recently closed. Names and dates only — routing, not a table. | SSE |
| **People pulse** | Recent activity: who answered, who changed their answer, identities newly created. Entry point to the directory (F-9). | SSE |
| **System** | One line, green/amber, tap for detail: deploy state and current SHA, stream health, last backup, probe status (PKT-10). Amber is a route to the detail, never a fix-it button. | SSE |

**"Happening now" is the answer to DEBATE-002's operator-under-load objection.** Landing on a
directory at 9pm was the complaint; landing on a dashboard whose first card is the event in progress
resolves it without changing what the design gate locks.

### Later (slots left deliberately open)

Named so the structure is designed against them, not built for them: **photos** (the shared-album
replacement, EV-010 — likely "recent uploads" plus a needs-review count), **calendar** (what's coming
across events and non-event things), **circles** (who's in what group), **friend activity** (once
accounts exist — and the SSE seam built here is what it arrives on, DEC-006), and **invitations
received** once the owner is not the only account.

The long-term shape this anticipates: as ronitnath.com becomes a hub (EV-009), the dashboard is the
one surface every role has, and role determines its card set. FRIEND's root is the same component
with a different set — which is the same composition idea as per-event panels, applied to roles.

## Rules

1. **Every card is a route.** If a card can mutate state, it has escaped its purpose. The one
   permitted exception is dismissing a "needs you" row, which is itself a routing decision.
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

- Does **"needs you"** persist dismissals, or recompute from state every time? Recomputing is simpler
  and self-healing; persisting allows "not now". Recommend recompute, revisit if it nags.
- Does the dashboard show **other people's** activity once friends exist, or stay the owner's own
  view? Decides whether the pulse card is a feed or a log. Later bet, but the card shape differs.
