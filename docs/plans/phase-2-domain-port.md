# Phase 2 — Domain port from events

Sources: `~/dev/personal/events` (authoritative, = nexus deploy @ bc79548).

Scope:
- Migrations 0011-0021 verbatim (identical base through 0010).
- `src/store/{people,events,event_links,schedule,attendance,segments}.rs`,
  handlers (event_public resolve()/build_view(), admin CRUD), templates,
  RSVP Solid islands, admin CLI subcommands (seed, mint-link, list-links,
  revoke-link, add-people, set-status, set-segment, set-invite,
  set-headcount).
- Templates re-skinned to docs/design.md; July 4th event page keeps its
  gold/fireworks poster theme as the first documented per-event theme.
- ICS export route ported from legacy repo (GET /events/{ref}/ics pattern).

Acceptance: feature parity with live events.ronitnath.com (event page,
RSVP, schedule tiers, segment RSVPs, headcount); test suite extended in
events' pattern; `cargo sqlx prepare -- --tests` cache committed;
adversarial visual review vs brief (agent-browser).
