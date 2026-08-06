---
product: ronitnath-universe
pitch: PITCH-001 — event platform + minimum identity
date: 2026-08-05
status: DRAFT — awaiting owner freeze (bound confirmation)
sources: BRIEF-001, DEBATE-001
debate: triggered (T3: identity + page-doc schemas, agent-API surface) → DEBATE-001
---

## Problem

The site is dormant and there is no path to create an event ("pickleball on Aug 15") — historically every event meant building a new codebase (EV-008, EV-013). The platform must make event creation trivial *without* losing what forced those rebuilds: genuinely different requirements and a specialized visual identity per event (EV-011).

## Bound (proposed — confirm or strike at freeze)

**3 owner review sessions** (SYS-DEC-001): ① this freeze, ② working software inspection, ③ pickleball-test acceptance on the real domain. Each ends continue / cut-scope / kill. *Optional upgrade: pin to a real event date if the owner wants to commit one — reality is the best deadline.* No time estimates anywhere.

## Solution shape

In `universe-ronitnath` (Leptos islands + Axum, EV-002), first stateful slice on the PostgreSQL seam:

1. **Minimum identity** — identity(kind=human) / account / credential / auth_factor / session / capability_link / audit. Guests are account-less identity rows (longitudinal people). Encrypted wire-ids from table one; links/sessions stay bearer tokens (EV-004). Single `authorize(actor, action)` seam even while the answer is always "owner" (DEBATE-001 C-2 guard).
2. **Agent creation surface (primary)** — HTTP/MCP API authenticated by owner-minted named, coarse-scoped bearer tokens; audit rows carry the token label. No agent identities (EV-009). The flow: owner tells agent about the event → agent creates it via API (EV-012).
3. **Modular event pages** — page = ordered doc of module instances, per-module props validated at write. Cut-one registry: hero, rich body, schedule (flat + keyed segments w/ optional per-segment RSVP), RSVP/status panel, photo-guided entry instructions, style wrapper. **Per-event visual identity is code**, written by the creating agent (starfield/fireworks precedent) — content in the doc, identity in the registry (DEBATE-001 C-6).
4. **Admin UI (refinement surface)** — tweak wording, manage/mint/revoke invite links, see RSVPs. Not the creation path.
5. **Data ground truth** — import of Housewarming + B24 + July 4th (people, attendance, content) is the **acceptance test of the data model**; export is the same path reversed. Live site's `app.db` snapshotted before cutover.
6. **Cutover** — ronitnath.com points at universe (presence + events); the dormant live site retires (its data preserved per 5).

## Acceptance (the pickleball test)

Owner tells an agent about a pickleball event → agent creates it via API, with its own styling → owner tweaks wording + mints invites in the UI → friend opens the link and RSVPs — on the real public ronitnath.com, with all three past events imported and rendering.

## Rabbit holes (named, avoided)

Prop-schema versioning (archived events freeze instead — C-7); grants/roles system (deferred to friend-accounts bet — C-2); per-segment payments/capacity (C-9); module marketplace dynamics (registry stays curated, in-code).

## No-gos

Friend account *onboarding* (schema-ready only), photos area (EV-010, own bet), calendar, circles, Telegram pings (C-10), E2EE, dashboard for this PM system.

## On freeze

Tag `pitch/ronitnath-universe-1`. Work packets derive from this document; deltas arrive as PRs against it, never edits.
