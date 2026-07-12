# Phase 4 — Guest accounts (link-claim + password)

Contract: 000-architecture.md §d, §g #1-3.

Scope:
- Migrations 0027_person_identity_links, 0028_people_recovery_email.
- Claim flow (`/e/{token}/claim` GET/POST): txn per §d — identity + guest
  account (purpose='guest') + membership + password factor
  (external_id='guest:{person_id}', never email) + optional recovery_email
  + person_identity_links + session.
- Site bin gains attach_session + per-handler CSRF on mutating routes
  (documented divergence); guest /login /logout (distinct from admin),
  /my dashboard, /my/events/{id}; `GuestScope` extractor (resolves OWNER
  account via person_identity_links).
- Token+session mismatch: content follows token, MismatchNote banner.
- Admin: /people/{id}/claim-status view + force-unlink (tombstone,
  re-claim allowed).

Acceptance: claim -> logout -> password login -> RSVP-as-self round-trip;
revoke-after-claim and re-mint leave login working; unknown-vs-revoked 404
parity kept; timing-oracle discipline (dummy-hash verify) kept; leak-matrix
extended with Guest viewer.
