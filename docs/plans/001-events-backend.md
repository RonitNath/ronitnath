# Events Backend Implementation Plan

## Scope

Implement the backend for the events system without touching frontend assets,
templates, CSS, or writing HTML.

Do not edit:

- `templates/**`
- `ui/**`
- event theme CSS
- SolidJS islands

Backend routes may return redirects, status codes, JSON, or plain-text machine
responses until SSR views are implemented later.

## Phases

1. Add backend dependencies and configuration.
2. Add SQLite database startup and migrations.
3. Implement domain models, stores, and services.
4. Implement token generation, hashing, and verification.
5. Wire Isoastra SSO entry through `/enter`.
6. Resolve viewers as anonymous, invitee, or admin.
7. Implement event, schedule, invitee, guest, script, QR, and capacity services.
8. Add backend-only route handlers for service access.
9. Add rate limiting, request hardening, CSRF/nonces, and audit logging.
10. Add backend tests and run verification.

## Dependencies

Add:

- `sqlx` with SQLite and migrations.
- `ulid` for resource ids.
- `time` or `chrono` for timestamps.
- `rand` for token generation.
- `hmac`, `sha2`, `subtle`, and `base64` for keyed token hashing.
- `tower-governor` for route-class rate limiting.
- `isoastra-auth` as a path dependency from `../isoastra/crates/isoastra-auth`.
- A QR code crate for server-side SVG generation.

## Configuration

Extend configuration with:

```text
database_url
public_base_url
token_secret
isoastra_issuer
isoastra_client_id
isoastra_client_secret
isoastra_redirect_uri
session_cookie_name
session_cookie_secure
admin_identity_ids
admin_emails
```

Secrets must be overridable by environment variables.

## Database

Create migrations for:

- `events`
- `event_schedule_items`
- `event_invitees`
- `event_invitee_guests`
- `event_message_scripts`
- `event_invitee_script_overrides`
- `event_message_log`
- `event_audit_log`

Create indexes for event visibility, token lookup, RSVP state, schedule order,
and audit/message history.

## Domain Modules

Add:

```text
src/db.rs
src/events/mod.rs
src/events/models.rs
src/events/store.rs
src/events/service.rs
src/events/tokens.rs
src/events/viewer.rs
src/events/capacity.rs
src/events/scripts.rs
src/events/qr.rs
src/events/errors.rs
```

Routes should use services instead of mutating tables directly.

## Token Rules

- Generate high-entropy raw tokens.
- Store only keyed hashes.
- Use purpose-separated hashes, such as `invite:v1:<token>`.
- Verify with constant-time comparison where practical.
- Support invitee token rotation.
- Support optional self-signup gate tokens.

## Auth Rules

- `/enter` starts Isoastra SSO and redirects back to `/events`.
- There is no public login link.
- Admin mode requires a valid Isoastra session plus a local allowlist match.
- Unauthorized Isoastra sessions render as normal public viewers.

## Capacity Rules

- Admins may exceed event capacity.
- Self-signup closes when confirmed public attendance reaches capacity.
- Public capacity display is clamped to the cap.
- Admin capacity display shows true count and overage.
- Waitlist behavior is deferred.

## Verification

Add backend tests for:

- Migration startup.
- Event CRUD and visibility.
- Invite token hashing and validation.
- RSVP and first-class guest updates.
- Party size enforcement.
- Self-signup capacity enforcement.
- Public capacity clamping.
- Admin over-cap behavior.
- Script rendering and copy logging.
- Audit logging.
- Viewer resolution.
