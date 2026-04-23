# Events

## Status

Proposed.

## Goal

Build an events system for this site that can serve polished public event
pages, personalized RSVP pages, and a fast admin workflow from the same route
surface.

The primary admin workflow is operational:

- Create and edit events quickly.
- Manage invitees and their guests.
- Copy personalized invite messages and RSVP links into external messaging
  apps.
- Track RSVP state, party size, dietary restrictions, notes, capacity, and
  message/link activity.
- Iterate on event-specific layouts and CSS with an AI coding agent.

The public workflow should not expose a login link or admin surface. Admin
access starts only by manually visiting `/enter`, which begins the Isoastra SSO
flow.

## Non-Goals

- Invitees do not need user accounts.
- Invitees do not sign in through Isoastra.
- This proposal does not include payment, ticketing, seating charts, or
  waitlist automation.
- This proposal does not require a full client-side admin application.
- This proposal does not make obscurity the security boundary. `/enter` may be
  discovered; authorization still depends on Isoastra identity plus a local
  admin allowlist.

## Route Model

Use a single route family and enrich SSR output based on permissions.

Public and invitee routes:

```text
GET  /events
GET  /events/:event_id_or_slug
GET  /events/:event_id_or_slug/r/:invite_token
POST /events/:event_id_or_slug/r/:invite_token
GET  /events/:event_id_or_slug/signup
POST /events/:event_id_or_slug/signup
GET  /events/:event_id_or_slug/ics
```

Hidden admin entry and auth routes:

```text
GET /enter
GET /auth/callback
GET /auth/logout
```

Admin mode does not live under a separate `/admin` namespace. When the request
has an authorized Isoastra session, the same `/events` and event detail routes
render an enriched operations UI. Anonymous users see only public event content.

## Viewer Model

Every request should resolve into one of three viewer modes:

```text
Viewer::Anonymous
Viewer::Invitee { invitee_id, event_id, token_scope }
Viewer::Admin { isoastra_identity_id, role }
```

`Viewer::Invitee` is produced by a valid RSVP token. The token is a scoped
bearer credential for exactly one invitee/event pair.

`Viewer::Admin` is produced by Isoastra SSO and a local admin authorization
check. Do not treat any Isoastra-authenticated user as an admin by default.
Maintain an allowlist by Isoastra identity id and/or verified email.

If a user completes SSO but is not authorized, render the normal public
experience. Do not show a public "admin only" page.

## Isoastra SSO

Use the existing `isoastra-auth` relying-party helper from `../isoastra` for:

- OIDC login, callback, and logout.
- PKCE/state/nonce handling.
- JWKS verification.
- RP session cookies.
- `require_auth` and `optional_auth` middleware.

`GET /enter` should be the only intentional login entrypoint. It should start
the Isoastra redirect flow and return to `/events` after login. Do not render a
login link in navigation or event pages.

## Template Engine

The current site uses Askama, which is type-safe and fast but compile-time
oriented. That is good for stable pages. Events need a faster iteration loop:
an AI coding agent should be able to add a new layout or CSS file and refresh
the page without adding Rust template structs for every design.

Recommendation:

- Keep Askama acceptable for stable existing pages.
- Use Tera for event pages and event layouts.
- Consider migrating the whole site to Tera later if the split becomes
  annoying.

Tera is a better fit for event iteration because it can load templates from a
glob, render from a serialized context, use inheritance/includes, and reload
changed or newly added template files in development.

Suggested layout:

```text
templates/events/index.html
templates/events/detail.html
templates/events/rsvp.html
templates/events/signup.html
templates/events/layouts/default.html
templates/events/layouts/cooking_social.html
templates/events/layouts/winter.html
templates/events/layouts/beach.html

static/event-themes/default.css
static/event-themes/cooking-social.css
static/event-themes/winter.css
static/event-themes/beach.css
```

Each event stores:

```text
layout_key
theme_css_path
theme_config_json
```

In development, reload Tera templates on request or through a file watcher. In
production, load templates once at startup and fail fast if any configured
template is invalid.

Use plain CSS files under `static/event-themes/` for event-specific art
direction. The main app can continue using vanilla-extract. Event themes should
be easy for a coding agent to create, inspect, and adjust without requiring a
frontend build pipeline for every visual iteration.

## Data Model

Use ULIDs for application resource ids. Use separate high-entropy random tokens
for RSVP and signup links. Store only token hashes.

### events

```text
id ulid primary key
slug nullable unique
title
subtitle nullable
summary nullable
details_markdown
location_name nullable
address nullable
map_url nullable
starts_at
ends_at
timezone
status draft | published | archived
visibility public | unlisted | invite_only
signup_mode invite_only | self_signup
self_signup_token_hash nullable
self_signup_requires_approval bool default false
attendee_cap nullable integer
display_capacity bool default false
layout_key
theme_css_path nullable
theme_config_json
notes_label default "Notes"
notes_caption nullable
dietary_label default "Dietary restrictions"
arrival_note_label default "Arrival timing"
arrival_note_caption nullable
rsvp_closes_at nullable
allow_rsvp_edits bool default true
created_by_isoastra_identity_id nullable
created_at
updated_at
```

### event_schedule_items

```text
id ulid primary key
event_id ulid
starts_at nullable
ends_at nullable
title
details nullable
location_name nullable
sort_order
created_at
updated_at
```

### event_invitees

```text
id ulid primary key
event_id ulid
display_name
email nullable
phone nullable
invite_token_hash unique
invite_token_version integer
party_size_limit integer
rsvp_status invited | opened | yes | no | maybe
arrival_note
dietary_restrictions
general_notes
notes_caption_snapshot nullable
personalized_script_key nullable
personalized_script_override nullable
sent_at nullable
opened_at nullable
responded_at nullable
created_at
updated_at
```

### event_invitee_guests

Guests and +1s are first-class rows, not JSON blobs.

```text
id ulid primary key
invitee_id ulid
display_name
attending bool default true
dietary_restrictions
general_notes
created_at
updated_at
```

### event_message_scripts

Script templates support the admin copy workflow.

```text
id ulid primary key
event_id ulid
key
label
body_template
sort_order
active bool
created_at
updated_at
```

Supported placeholders should begin small:

```text
{{ invitee.name }}
{{ event.title }}
{{ event.date }}
{{ event.location }}
{{ rsvp_url }}
{{ signup_url }}
```

The placeholder model should be structured so more fields can be added later
without changing stored script text.

### event_invitee_script_overrides

```text
id ulid primary key
invitee_id ulid
script_id ulid
body_template
created_at
updated_at
```

### event_message_log

Track copy actions separately from actual sends. Copying a script into the
clipboard does not prove it was delivered in the external messaging app.

```text
id ulid primary key
event_id ulid
invitee_id ulid nullable
script_id ulid nullable
actor_isoastra_identity_id nullable
kind copied | sent
recipient nullable
rendered_hash nullable
idempotency_key nullable
created_at
```

### event_audit_log

```text
id ulid primary key
event_id ulid nullable
actor_isoastra_identity_id nullable
action
metadata_json
created_at
```

## RSVP Page

The personalized RSVP page should be SSR-first and work without JavaScript.
SolidJS should enhance the +1 manager only.

Fields:

- Attendance: yes, no, maybe.
- Primary invitee dietary restrictions.
- Arrival timing note, for messages like "I may arrive late" or "I may come
  early."
- General notes with event-configured label and caption.
- First-class guest/+1 manager.

Guest fields:

- Name.
- Attending.
- Dietary restrictions.
- General notes.

All text inputs should be generous but bounded. Suggested limits:

```text
arrival_note: 500 chars
dietary_restrictions: 1000 chars
general_notes: 4000 chars
guest dietary_restrictions: 1000 chars
guest general_notes: 1000 chars
```

For a cooking social, event notes could be configured as:

```text
notes_label = "What are you thinking of bringing?"
notes_caption = "Optional. If you already know, add a dish, ingredient, drink, or equipment you might bring."
```

The caption is event data, not code.

## Self-Signup and QR Codes

Some events should allow people to sign themselves up by scanning a QR code or
visiting a public signup link.

Self-signup route:

```text
/events/:event_id_or_slug/signup
```

Optional semi-private signup route:

```text
/events/:event_id_or_slug/signup?t=<signup_token>
```

Self-signup creates an `event_invitees` row. Any +1s entered during signup
create `event_invitee_guests` rows.

Support three QR contexts:

- Invitee QR: points to that invitee's RSVP token URL.
- Self-signup QR: points to the event signup URL.
- Admin preview QR: shown only in admin-enriched views.

Prefer server-generated SVG QR codes so printing and screenshotting work
without an external service.

## Capacity

Capacity applies to public/self-signup behavior, not admin authority.

Rules:

- Admins can always add invitees or guests beyond the cap.
- Self-signup closes when confirmed public attendance reaches the cap.
- Public capacity display is clamped to the cap.
- Admin views show the true count and over-cap amount.

Example:

```text
attendee_cap = 30
actual confirmed = 34
public display = "30 / 30"
admin display = "34 / 30 (+4 over)"
```

Do not leak that the admin has exceeded capacity on public pages.

Confirmed attendance should count:

- The primary invitee when `rsvp_status = yes`.
- Guests where `attending = true`.

`maybe` should not block capacity in v1, but it should be visible in admin
counts.

Waitlist support is deferred. The data model should leave room for a future
`waitlist` RSVP state and `waitlist_position`.

## Admin-Enriched Events Page

When authenticated as an authorized admin, `/events` should become a dense
operations console.

Event list columns:

- Title.
- Date/time.
- Status.
- Visibility.
- Signup mode.
- Confirmed/capacity.
- Maybe.
- Pending.
- Unsent.
- Theme/layout.
- Recent activity.

Quick actions:

- Create event.
- Copy public link.
- Show self-signup QR.
- Duplicate event.
- Publish/unpublish.
- Archive.

Event detail should combine editor, dashboard, and preview:

- Header: title, status, date, visibility, capacity, primary actions.
- Main area: event details, schedule, public preview, scripts.
- Side area: counts, QR actions, recent activity.
- Main table: invitees and guests.

Invitee table columns:

- Name.
- Email/phone.
- RSVP status.
- Party size.
- Dietary/note indicators.
- Link opened.
- Last copied/sent.
- Script.
- Copy message.
- Copy link.
- Show QR.
- Regenerate link.
- Mark sent.

Rows should expand to show guest details. Inline editing is preferred where it
keeps the flow fast.

## Copy Workflow

The primary messaging workflow is clipboard-based, not provider-integrated.

For each invitee, the admin can:

- Copy a rendered generic script plus RSVP URL.
- Copy a selected alternate script.
- Copy only the RSVP link.
- Use an invitee-specific script override.
- Show a QR code.

Clipboard actions require a tiny client-side enhancement because the Clipboard
API is browser-side. This can be a small island or minimal JavaScript module.
It should not require the admin surface to become a SPA.

Copying should create `event_message_log.kind = copied`. This records workflow
activity without claiming the message was delivered.

## SolidJS Scope

Use SolidJS only where dynamic browser behavior is valuable:

- RSVP +1 manager.
- Clipboard copy actions.
- Optional QR modal.
- Optional CSV import preview.
- Optional schedule drag/reorder later.

Core admin and RSVP flows should remain SSR-first and work through normal form
posts.

## Security and Hardening

Add `tower-governor` with route-class-specific policies.

Suggested rate-limit classes:

```text
Global:
- moderate per-IP request cap
- generous enough for normal asset and page loads

/enter:
- strict per IP
- low burst

/auth/callback:
- moderate, allowing OAuth retries

RSVP token pages:
- moderate per IP
- stricter per token hash for POST updates

Self-signup:
- strict per IP and signup token/event

POST mutations:
- strict per admin session or invite token
```

Additional hardening:

- Request body size limits.
- CSRF tokens for admin forms.
- Token-bound form nonce for RSVP and signup posts.
- Hash RSVP and signup tokens with an HMAC or keyed hash.
- Do not store raw RSVP tokens.
- Generic errors for invalid RSVP tokens.
- No public route that reveals whether an email is invited.
- No public invitee listing unless explicitly configured.
- Escape all SSR fields.
- Sanitize Markdown if event details support Markdown.
- Security headers: CSP, `X-Content-Type-Options`, `Referrer-Policy`, and
  `frame-ancestors`.
- SameSite=Lax secure cookies for Isoastra sessions.
- Audit admin mutations, especially capacity overrides and token regeneration.
- Add idempotency keys for any future real send operation.
- Add DB indexes for `event_id`, `token_hash`, `status`, and `starts_at`.
- Paginate large invitee tables.
- Ensure static asset requests do not consume strict dynamic route limits.

## Deferred Features

- Waitlist automation.
- Real email/SMS sending integrations.
- Seating/table assignment.
- Check-in mode.
- Payment or ticketing.
- Public attendee lists.
- Full SPA admin console.
- Multi-event series support.

## Implementation Plan

1. Add SQLite/sqlx persistence, migrations, and repository modules.
2. Add event, schedule item, invitee, guest, script, message log, and audit
   tables.
3. Add Isoastra OIDC wiring and hidden `/enter`.
4. Add viewer resolution for anonymous, invitee token, and admin.
5. Replace the `/events` placeholder with Tera-backed event routes.
6. Build admin-enriched SSR views for `/events` and event detail.
7. Build RSVP token pages with first-class guest management.
8. Add the SolidJS +1 manager island.
9. Add clipboard script/link copying.
10. Add self-signup and QR code support.
11. Add capacity enforcement and public display clamping.
12. Add tower-governor, body limits, CSRF, and security headers.
13. Add event theme/layout loading and development reload support.
