# Phase 3 — Visibility (circles + levels)

Contract: 000-architecture.md §a (0022-0026 + 0033), §b, §f.

Scope:
- Migrations 0022_circles, 0023_circle_members, 0024_audience_policies,
  0025_audience_circle_grants, 0026_audience_person_overrides,
  0033_accounts_purpose.
- `src/access/level.rs`: pure `level_for()` + `level_for_direct_hit()`,
  unit-tested without DB.
- `Viewer` extractor (`src/auth/viewer.rs`) + `Viewer::combine_with_link`.
- Generalize chokepoints: `Event::view_for(level)`,
  `Store::list_schedule(.., level)`; app-level invariant: create_event
  inserts its audience_policies row (hidden) in the same txn.
- Circles CRUD + audience editor (admin UI) + `set-audience` CLI.
- `require_single_account` -> `purpose='primary'` (owner account cached in
  AppState at startup).

Acceptance: leak-matrix tests — every viewer class x level x subject type,
busy-block rendering, summary floor on direct token hit, exclude-beats-all,
multi-circle max. No handler re-derives policy.
