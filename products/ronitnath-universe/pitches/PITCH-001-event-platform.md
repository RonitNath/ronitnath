---
product: ronitnath-universe
pitch: PITCH-001 — event platform + minimum identity
date: 2026-08-05
status: FROZEN 2026-08-05 (owner: "pitch looks fine" + expressibility amendment + packet-layer addition; bound = 3 review sessions, session ① spent at freeze)
sources: BRIEF-001, DEBATE-001
debate: triggered (T3: identity + page-doc schemas, agent-API surface) → DEBATE-001
---

## Problem

The site is dormant and there is no path to create an event ("pickleball on Aug 15") — historically every event meant building a new codebase (EV-008, EV-013). The platform must make event creation trivial *without* losing what forced those rebuilds: genuinely different requirements and a specialized visual identity per event (EV-011).

## Bound (amended by delta-1)

**5 owner review sessions** (SYS-DEC-001): ① freeze (spent), ② design gate (layered Penpot + stories, `system/design-gate.md`), ③ data gate (domain model + seams, `system/data-gate.md`), ④ working-software inspection, ⑤ pickleball-test acceptance on the real domain. Each ends continue / cut-scope / kill. *Standing offer: pin to a real event date if the owner commits one — reality is the best deadline.* No time estimates anywhere.

## Deltas

- **delta-1** (2026-08-05, owner, session ① follow-on): bound restructured 3→5 sessions — design gate and data gate inserted before build (SYS-DEC-003). No scope change.
- **delta-3** (2026-08-05, owner at session ② design gate; EV-015/016/017, DEC-005): **authoring-model pivot.** Copy lives in a per-event `copy.toml` the owner edits directly, not in an inline console editor; structural changes go through the coding agent; the event's **admin panel is per-event** like its invite pages. §4 rewritten below. Import loses its UI (EV-017). The flow addendum `FLOWS-001` is added retroactively per SYS-DEC-004 and summarized below; it re-freezes with this pitch. Net scope: inline copy-editing UI removed, per-event admin composition + a copy-source pipeline (PKT-11) added.

## Solution shape

In `universe-ronitnath` (Leptos islands + Axum, EV-002), first stateful slice on the PostgreSQL seam:

1. **Minimum identity** — identity(kind=human) / account / credential / auth_factor / session / capability_link / audit. Guests are account-less identity rows (longitudinal people). Encrypted wire-ids from table one; links/sessions stay bearer tokens (EV-004). Single `authorize(actor, action)` seam even while the answer is always "owner" (DEBATE-001 C-2 guard).
2. **Agent creation surface (primary)** — HTTP/MCP API authenticated by owner-minted named, coarse-scoped bearer tokens; audit rows carry the token label. No agent identities (EV-009). The flow: owner tells agent about the event → agent creates it via API (EV-012).
3. **Modular event pages** — page = ordered doc of module instances, per-module props validated at write. Cut-one registry: hero, rich body, schedule (flat + keyed segments w/ optional per-segment RSVP), RSVP/status panel, photo-guided entry instructions, style wrapper. **Per-event visual identity is code**, written by the creating agent (starfield/fireworks precedent) — content in the doc, identity in the registry (DEBATE-001 C-6).
4. **Admin UI (runtime-data surface)** — *rewritten by delta-3.* The console owns only what exists at runtime: RSVPs and guests, minting/labelling/revoking invite links, publish state, and the cross-event people view. It does **not** edit wording (a per-event `copy.toml`, edited directly) and does not edit structure (the coding agent). Its **per-event admin panel is composed per event** from the shared component library plus event-specific components, exactly like an invite page; the canonical surfaces are sign-in, the event index, people, and link management (DEC-005).
5. **Data ground truth** — the **expressibility test** (owner amendment at freeze): everything Housewarming + B24 + July 4th contained — people, attendance, content — must be *representable* in the new model, which is the fourth distinct data model to hold this data. The import tooling itself may be rough and one-shot; smoothness is not the bar. Export is first-class. Live site's `app.db` snapshotted before cutover.
6. **Cutover** — ronitnath.com points at universe (presence + events); the dormant live site retires (its data preserved per 5).

## Flows (summary — full addendum in `flows/FLOWS-001-event-platform.md`, added by delta-3)

| ID | Flow | Actor | Surface |
| --- | --- | --- | --- |
| F-1 | Sign in | OWNER | web |
| F-2 | Create an event by talking to the agent | OWNER | mixed |
| F-3 | Change wording — edit the event's `copy.toml` | OWNER | mixed |
| F-4 | Change structure — talk to the agent | OWNER | outside-web |
| F-5 | Mint and share invites, setting what each shows | OWNER | web |
| F-6 | Watch responses on the event's own admin panel | OWNER | web (per-event) |
| F-7 | Open an invite and respond | GUEST | web (per-invite) |
| F-8 | Arrive without a link | VISITOR | web |
| F-9 | Look up a person across events | OWNER | web |

Not flows: expressing past events (one-time, agent-mediated — EV-017); friend signup; photos/calendar/circles. AGENT has no UI, so no flows of its own; it is a mediator inside F-2 and F-4.

## Acceptance (the pickleball test)

Owner tells an agent about a pickleball event → agent authors it (page, admin panel, identity-as-code, `copy.toml`) and creates it via API → owner fixes a line of wording in the copy file and it shows up live → owner mints invites in the console → friend opens the link and RSVPs — on the real public ronitnath.com, with all three past events' recovered data expressed in the new model and rendering (import may be hand-cranked, and has no UI).

## Rabbit holes (named, avoided)

Prop-schema versioning (archived events freeze instead — C-7); grants/roles system (deferred to friend-accounts bet — C-2); per-segment payments/capacity (C-9); module marketplace dynamics (registry stays curated, in-code).

## No-gos

Friend account *onboarding* (schema-ready only), photos area (EV-010, own bet), calendar, circles, Telegram pings (C-10), E2EE, dashboard for this PM system.

## On freeze

Tag `pitch/ronitnath-universe-1`. Work packets derive from this document per `system/work-packets.md` — including the mandatory peripheral coverage (actor journeys, empty/error/edge states, UX artifacts) — and live in the code repo. Deltas arrive as PRs against this pitch, never edits.
