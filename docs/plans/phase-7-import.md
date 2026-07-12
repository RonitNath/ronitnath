# Phase 7 — Prod data migration (guarded)

Contract: 000-architecture.md §f (PK preservation, backfill rules).

Scope:
- `admin import-legacy-db <path>`: into EMPTY freshly-migrated DB, BEFORE
  owner signup; PKs verbatim (token_hash-only resolver makes /e/{token}
  continuity structural); legacy sessions dropped; audience backfill:
  person-bound private links -> audience_person_overrides(include, full);
  any public link -> public_level='summary' else 'hidden';
  accounts.purpose='primary' for the imported owner account.
- `admin verify-import`: row counts per table vs source, every
  event_links.token_plain resolves through the real resolver, policy row
  per event.

Acceptance: dry run against a COPY of nexus prod app.db (copy aside first,
durable-data rules); verify-import green; spot-check 5 real invite URLs
against the locally-running site bin.
