---
id: DEC-007
date: 2026-08-05
source: owner ruling on DEBATE-002 escalation 1 (C-8 ↔ C-9)
---
Chose **per-event extensions may own scoped tables and commands, but not routes, authorization rules, or SSE topics**, resolving the cohesion/variance collision.

**Why variance won the persistence question**: H6 cited artifacts, not arguments — `0020_segment_paid_attended.up.sql` and an admin media route from the archived predecessors. Those events *already* needed persistence and commands beyond a fixed RSVP model. A boundary forbidding per-event tables would reproduce exactly the condition that forced three rebuilds (EV-011, EV-013), which is the condition this bet exists to remove.

**Why cohesion still won the rest**: routes, authorization and SSE topics are where N unrelated applications sharing a database would actually diverge. Held centrally, an extension cannot invent an auth model, escape its event scope on the stream, or claim a URL.

| An extension MAY | An extension MAY NOT |
| --- | --- |
| Register guest-facing and admin-facing presentation components | Add routes or claim URLs |
| Ship theme assets and its own visual identity as code | Define authorization rules — everything goes through `authorize(actor, action)` |
| Own tables **scoped to its event**, for state the platform doesn't model | Define private SSE topics or subscribe outside its event scope |
| Expose commands operating on its own tables | Alter platform tables, or change RSVP/capability semantics |

**Consequences:**

1. The registry (PKT-03) serves both surfaces and now also gates a declared extension manifest: what tables it owns, what commands it exposes.
2. Extension-owned tables are in scope for export (pitch §5) — an event's payment or attendance-tracking state is the owner's data like any other.
3. The migration story is per-extension and lands on the data gate: an extension's tables come and go with its event, and an archived event's tables must not block a platform migration.
4. Cohesion's counter-proposal (`EventAdminSnapshot`, canonical commands only — H5-cohesion-4) is **not** adopted, but its underlying point stands: platform facts (responses, invites, publish state) are read and written only through canonical paths, never by extension SQL against platform tables.

**What would change this**: if two events' extension tables turn out to be the same table with different names, the modelled-centrally line has moved and the platform should absorb it. Revisit after two real events.
