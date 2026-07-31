# Phase 6 — Calendar

Contract: 000-architecture.md §a (0029-0030), read-time union decision.

Scope:
- Migrations 0029_calendar_entries, 0030_calendar_feed_tokens.
- /calendar (site bin): per-viewer month/list, query-time union of events +
  standalone entries through the same audience machinery;
  `CalendarEntry::view_for(level)` chokepoint; busy renders as anonymous
  block.
- Admin: calendar entries CRUD + audience editor reuse;
  /people/{id}/calendar-feed mint/revoke + `mint-calendar-feed` CLI.
- /calendar/{token}.ics per-person feed: level-respecting (busy -> opaque
  VEVENT, summary -> title only, full -> details).

Acceptance: same month rendered as anonymous/link-holder/guest/owner shows
exactly the leak-matrix-prescribed content; ICS respects levels; revoked
feed token 404s. Sonnet leg for calendar UI.
