# Dynamic migration plan

## Delivered in this rebuild

The fresh `web_template` `public-site` derivation serves `/` and `/calendar`
with the exact current anonymous copy and existing night-sky CSS, with Solid
islands for the theme toggle and responsive calendar chrome. Server fallbacks
remain readable without JavaScript.

## Deferred deliberately

The prior application has durable SQLite-backed event, invite, guest-account,
photo, ICS and mesh-only admin surfaces. `public-site` intentionally has no
persistence, identity door, or operator-admin product routes. They are absent
from this branch, not silently bridged to production:

- event/invite pages and RSVP APIs (`/e/{token}`, `/api/e/{token}`);
- guest claim/login and `/my` views/feeds;
- calendar event data, audience filtering, and ICS feeds;
- photo upload/variants; and
- authenticated mesh-only admin, people, circles, audit and mutation routes.

## Follow-up sequence

1. Start from `oidc-app` or compose the approved browser boundary, then port
   product migrations into the product-owned history; preserve deployed SQLite
   data and never edit applied migrations.
2. Port events, audience, calendar, invite and ICS reads behind feature-owned
   routers. Add direct anonymous/session/capability route tests before writes.
3. Port guest claim and RSVP with session/CSRF policy, then photos with bounded
   upload processing and private static scopes.
4. Mount the admin listener only on its separate operator/mesh boundary and
   port its capability policy, audit trail and mutation tests.
5. Regenerate SQLx metadata with `cargo sqlx prepare -- --tests` after every
   checked query. Preserve LF-only SQL/query-cache files: line-ending changes
   alter SQLx query checksums without changing query semantics.

No deploy or production-data connection is part of this source rebuild.
