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

- **delta-7** (2026-08-06, owner at design-gate sign-off; EV-023/024/025/026, DEC-012, DEC-009 amended): **session ② closes.** Three scope effects. **The nickname is removed from the bet** (EV-023) — it is a per-event affordance, so by DEC-007 it is extension territory, not a platform column; `contact` survives without it and pages greet by name. **The "answering as" banner is removed** (EV-024) — a bearer link cannot detect forwarding, so DEC-009's prescribed warning was fired by a check that cannot exist; the invite headline is `{person_name}'s invite to {event_title}`, unconditionally, which closes DEBATE-002 C-11 without any detection. **The design language is fixed as DEC-012** (five type sizes, larger-never-heavier, no eyebrows, no explanatory prose, no route chips). Net scope: a small **cut**. The owner's caveat that the pickleball event's own design still needs focused work lands at session ⑤, not here.
- **delta-1** (2026-08-05, owner, session ① follow-on): bound restructured 3→5 sessions — design gate and data gate inserted before build (SYS-DEC-003). No scope change.
- **delta-6** (2026-08-05, owner rulings; EV-021/EV-022, DEC-011, DASH-001): **role-resolved root, no `/app` split** — `/` renders the personal dashboard for the owner and presence for a visitor; `/landing` always reaches presence; `/auth` holds sign-in and later register; events live at `/e/{event}`, which role-resolves to the admin panel. The **personal dashboard** (four cards, present-event keyed on event **end time**) and **accounts management** are new surfaces — F-12/F-13, PKT-17/18. Owner-authored data is confined to `contact`, never `identity` (EV-021), and the people directory is renamed **contacts**.
- **delta-5** (2026-08-05, DEBATE-002 + owner rulings): the 8-lens panel confirmed five defects and closed two collisions. **DEC-007** — per-event extensions may own event-scoped tables and commands, not routes/authorization/SSE topics. **DEC-008** — retain what people *answered* (append-only response log) and whether they *showed* (owner-recorded), **not** what they were shown: identity-as-code stands and page reconstruction is knowingly given up. New **EventRelease** binds an event to its deployed code (F-2's long-open deploy seam). SSE weakens from replay to notification-plus-resync. New flow **F-11** closes an event and records attendance. Copy gains an optimistic precondition but no history.
- **delta-4** (2026-08-05, owner at flow-addendum review; EV-018/019/020, DEC-006): **copy is database rows edited in the console, not a file** — delta-3's TOML mechanism is superseded (its intent isn't). **SSE becomes the default update transport** for copy→open pages, RSVPs→dashboard, identities→directory (DEC-006); the operational posture for it lands on the data gate, where long-lived streams collide with rolling zero-downtime upgrades. **Per-invite composition is deferred out of the bet** (EV-020, supersedes EV-014 and packets delta-2) — a net scope *cut*: minting has no configuration step. **Contacts + nicknames** added (EV-019): links are name-slug+hash, pages greet by nickname, copy-link buttons persist a copied count. **Base screen = the people directory**, with identity CRUD as F-10. Identity is **ember** (DEC-004 resolved).
- **delta-3** (2026-08-05, owner at session ② design gate; EV-015/016/017, DEC-005): **authoring-model pivot.** Copy lives in a per-event `copy.toml` the owner edits directly, not in an inline console editor; structural changes go through the coding agent; the event's **admin panel is per-event** like its invite pages. §4 rewritten below. Import loses its UI (EV-017). The flow addendum `FLOWS-001` is added retroactively per SYS-DEC-004 and summarized below; it re-freezes with this pitch. Net scope: inline copy-editing UI removed, per-event admin composition + a copy-source pipeline (PKT-11) added.

## Solution shape

In `universe-ronitnath` (Leptos islands + Axum, EV-002), first stateful slice on the PostgreSQL seam:

1. **Minimum identity** — identity(kind=human) / **contact** (owner-authored nickname + details, delta-4) / account / credential / auth_factor / session / capability_link / audit / **response log** (append-only) / **attendance** (owner-recorded) / **event_release** (delta-5). Guests are account-less identity rows (longitudinal people). Encrypted wire-ids from table one; links/sessions stay bearer tokens (EV-004). Single `authorize(actor, action)` seam even while the answer is always "owner" (DEBATE-001 C-2 guard).
2. **Agent creation surface (primary)** — HTTP/MCP API authenticated by owner-minted named, coarse-scoped bearer tokens; audit rows carry the token label. No agent identities (EV-009). The flow: owner tells agent about the event → agent creates it via API (EV-012).
3. **Modular event pages** — page = ordered doc of module instances, per-module props validated at write. Cut-one registry: hero, rich body, schedule (flat + keyed segments w/ optional per-segment RSVP), RSVP/status panel, photo-guided entry instructions, style wrapper. **Per-event visual identity is code**, written by the creating agent (starfield/fireworks precedent) — content in the doc, identity in the registry (DEBATE-001 C-6).
4. **Admin UI (runtime-data surface)** — *rewritten by delta-3, amended by delta-4.* The console owns everything that exists at runtime: the **contacts CRM** (identities, contacts, nicknames, cross-event history, CRUD — owner-only, permanently, EV-021), event copy, invitee lists and their auto-minted links, RSVPs, publish state. It does **not** edit structure — that is the coding agent, in a terminal. Its **per-event admin panel is composed per event** from the shared component library plus event-specific components; canonical surfaces are sign-in, people, event index, copy, and invitees (DEC-005). Owner-facing surfaces **stream** (DEC-006): the dashboard updates as guests answer, without a refresh.
5. **Data ground truth** — the **expressibility test** (owner amendment at freeze): everything Housewarming + B24 + July 4th contained — people, attendance, content — must be *representable* in the new model, which is the fourth distinct data model to hold this data. The import tooling itself may be rough and one-shot; smoothness is not the bar. Export is first-class. Live site's `app.db` snapshotted before cutover.
6. **Cutover** — ronitnath.com points at universe (presence + events); the dormant live site retires (its data preserved per 5).

## Flows (summary — full addendum in `flows/FLOWS-001-event-platform.md`; signed off 2026-08-05)

| ID | Flow | Actor | Surface |
| --- | --- | --- | --- |
| F-1 | Sign in | OWNER | web |
| F-2 | Create an event by talking to the agent | OWNER | mixed |
| F-3 | Edit an event's copy — console page, DB-backed, streamed to open pages | OWNER | web |
| F-4 | Change structure — talk to the agent | OWNER | terminal |
| F-5 | Invite people and share links — auto-minted from a live invitee list | OWNER | web |
| F-6 | Watch responses come in live | OWNER | web (per-event) |
| F-7 | Open an invite and respond | GUEST | web |
| F-8 | Arrive without a link | VISITOR | web |
| F-9 | **People directory — the base screen** | OWNER | web |
| F-10 | Manage an identity (create / edit / merge / archive) | OWNER | web |
| F-11 | Close an event and record who came | OWNER | web |
| F-12 | Land on the dashboard and route from it | OWNER | web |
| F-13 | Manage accounts | OWNER | web |

Not flows: expressing past events (one-time, agent-mediated — EV-017); friend signup; photos/calendar/circles. AGENT has no UI, so no flows of its own; it is a mediator inside F-2 and F-4.

## Acceptance (the pickleball test)

Owner tells an agent about a pickleball event → agent authors it (page, admin panel, identity-as-code) and creates it via API with its copy → owner adds people to the invitee list and their links mint themselves → owner fixes a line of wording on the copy page **and it changes on a page already open** → owner works down the list copying links, and can see which he's already sent → friend opens their link, is greeted by nickname, and RSVPs **and the owner's dashboard moves without a refresh** — on the real public ronitnath.com, with all three past events' recovered data expressed in the new model and rendering (import may be hand-cranked, and has no UI).

## Rabbit holes (named, avoided)

Prop-schema versioning (archived events freeze instead — C-7); grants/roles system (deferred to friend-accounts bet — C-2); per-segment payments/capacity (C-9); module marketplace dynamics (registry stays curated, in-code); **per-invite page composition** (delta-4/EV-020 — the direction stands, the build doesn't; composition is per event in this bet); **WebSockets** (DEC-006 — SSE covers every server→client case here, and client-streaming arrives with friend accounts).

## No-gos

Friend account *onboarding* (schema-ready only), photos area (EV-010, own bet), calendar, circles, Telegram pings (C-10), E2EE, dashboard for this PM system.

## On freeze

Tag `pitch/ronitnath-universe-1`. Work packets derive from this document per `system/work-packets.md` — including the mandatory peripheral coverage (actor journeys, empty/error/edge states, UX artifacts) — and live in the code repo. Deltas arrive as PRs against this pitch, never edits.
