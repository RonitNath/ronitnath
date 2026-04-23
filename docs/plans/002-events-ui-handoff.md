# Events UI Handoff

## Scope

This handoff is for implementing the UI on top of the completed events backend.
It intentionally does not prescribe layouts, visual design, animations, themes,
or page composition.

The backend currently exposes service-backed JSON, SVG, calendar, and auth
routes. The UI layer can replace or augment these responses with SSR templates
and small islands as needed.

## Constraints

- Keep `/enter` hidden. Do not add a visible login link or public admin prompt.
- Admin UI should be permission-enriched content on `/events` and event routes,
  not a separate public `/admin` area.
- Invitees do not sign in. Their RSVP token is their scoped credential.
- Do not expose admin-only data in public HTML, public JSON, or bootstrap data.
- Public capacity display must be clamped to the attendee cap.
- Do not reveal that admin-added attendees have exceeded capacity.
- Invalid RSVP tokens should produce generic not-found behavior.

## Current Backend Files

Core backend modules:

```text
src/events/mod.rs
src/events/models.rs
src/events/service.rs
src/events/store.rs
src/events/tokens.rs
src/events/viewer.rs
src/events/capacity.rs
src/events/scripts.rs
src/events/qr.rs
src/events/errors.rs
src/db.rs
```

Database migration:

```text
migrations/202604220001_events.sql
```

Planning docs:

```text
docs/proposals/001-events.md
docs/plans/001-events-backend.md
```

## Viewer Modes

The backend models three viewer modes:

```text
Viewer::Anonymous
Viewer::Invitee { invitee_id, event_id }
Viewer::Admin { isoastra_identity_id, role }
```

Admin mode is resolved through Isoastra RP session data plus a local allowlist
from config. Unauthorized Isoastra sessions are treated as anonymous.

## Auth Entry

Hidden admin entrypoint:

```text
GET /enter
```

If auth is ready, this redirects to:

```text
/auth/login?return_to=%2Fevents
```

Isoastra callback/logout routes are wired by `isoastra-auth`:

```text
GET /auth/callback
GET /auth/logout
```

The backend may disable auth routes at startup if Isoastra JWKS is unavailable.
In that case `/enter` returns `503`.

## Existing Route Surface

Public or permission-sensitive route placeholders:

```text
GET  /events
GET  /events.json
POST /events

GET  /events/{event_ref}.json
POST /events/{event_ref}
POST /events/{event_ref}/publish
POST /events/{event_ref}/archive

GET  /events/{event_ref}/capacity.json
GET  /events/{event_ref}/ics

POST /events/{event_ref}/schedule
POST /events/{event_ref}/invitees
POST /events/{event_ref}/scripts

POST /events/{event_ref}/signup
GET  /events/{event_ref}/qr/signup.svg

GET  /events/{event_ref}/r/{token}
POST /events/{event_ref}/r/{token}
```

Current handlers return JSON, SVG, calendar text, redirects, or status codes.
They do not render new HTML yet.

## Route Behavior

`GET /events` still renders the existing placeholder template. UI work can
replace this with a permission-aware SSR view.

`GET /events.json` returns:

```json
{
  "viewer": "...",
  "events": []
}
```

Anonymous users receive only published public events. Admin users receive all
events.

`GET /events/{event_ref}.json` returns event data and schedule data if the
viewer can access the event.

`POST /events` requires admin. It creates a draft event.

`POST /events/{event_ref}` requires admin. It updates event fields.

`POST /events/{event_ref}/publish` requires admin.

`POST /events/{event_ref}/archive` requires admin.

`POST /events/{event_ref}/invitees` requires admin. It creates an invitee and
returns the raw RSVP token and rendered RSVP URL. The raw token is shown only in
this response because the database stores only its hash.

`GET /events/{event_ref}/r/{token}` resolves an invitee token, marks the link
opened if applicable, and returns invitee plus guest data.

`POST /events/{event_ref}/r/{token}` updates RSVP fields and replaces that
invitee's guest rows.

`POST /events/{event_ref}/signup` creates an invitee through self-signup if the
event allows it and capacity is available.

`GET /events/{event_ref}/capacity.json` returns public-safe capacity data.

`GET /events/{event_ref}/qr/signup.svg` returns a server-generated SVG QR code
for the event signup URL.

`GET /events/{event_ref}/ics` returns a minimal calendar file.

## Important Data Structures

Event fields include:

```text
id
slug
title
subtitle
summary
details_markdown
location_name
address
map_url
starts_at
ends_at
timezone
status
visibility
signup_mode
attendee_cap
display_capacity
layout_key
theme_css_path
theme_config_json
notes_label
notes_caption
dietary_label
arrival_note_label
arrival_note_caption
rsvp_closes_at
allow_rsvp_edits
```

Invitee fields include:

```text
id
event_id
display_name
email
phone
party_size_limit
rsvp_status
arrival_note
dietary_restrictions
general_notes
notes_caption_snapshot
personalized_script_key
personalized_script_override
sent_at
opened_at
responded_at
```

Guest fields include:

```text
id
invitee_id
display_name
attending
dietary_restrictions
general_notes
```

Script fields include:

```text
id
event_id
key
label
body_template
sort_order
active
```

## RSVP Inputs

The RSVP update payload is:

```json
{
  "rsvp_status": "yes",
  "arrival_note": "I may be late",
  "dietary_restrictions": "No shellfish",
  "general_notes": "I can bring something",
  "guests": [
    {
      "id": null,
      "display_name": "Guest Name",
      "attending": true,
      "dietary_restrictions": "",
      "general_notes": ""
    }
  ]
}
```

Valid RSVP statuses:

```text
invited
opened
yes
no
maybe
```

The UI should normally present only user-meaningful choices for RSVP. Backend
state values like `invited` and `opened` are lifecycle states.

Limits enforced by backend:

```text
arrival_note: 500 chars
dietary_restrictions: 1000 chars
general_notes: 4000 chars
guest dietary_restrictions: 1000 chars
guest general_notes: 1000 chars
```

Party size limit is enforced as:

```text
primary invitee + attending guests <= party_size_limit
```

## Self-Signup

Self-signup payload:

```json
{
  "display_name": "Name",
  "email": "person@example.com",
  "phone": null,
  "rsvp": {
    "rsvp_status": "yes",
    "arrival_note": "",
    "dietary_restrictions": "",
    "general_notes": "",
    "guests": []
  }
}
```

If an event has a signup token, include it as:

```text
/events/{event_ref}/signup?t=<token>
```

Self-signup can fail when:

- The event is not configured for self-signup.
- The signup token is missing or invalid.
- Capacity has been reached.

## Capacity Display

Capacity response shape:

```json
{
  "capacity": {
    "confirmed": 34,
    "cap": 30,
    "public_confirmed": 30,
    "over_cap": 4,
    "self_signup_open": false
  }
}
```

Public UI must use `public_confirmed`, not `confirmed`, when displaying counts.
Admin UI may use `confirmed` and `over_cap`.

## Script Copy Workflow

Backend services support message scripts and copy logging, but route coverage is
minimal today. The UI agent may need to add backend route handlers for:

- Rendering a script for an invitee.
- Logging a copy action.
- Copying only an RSVP link.
- Managing per-invitee script overrides.

Supported placeholders in script rendering:

```text
{{ invitee.name }}
{{ event.title }}
{{ event.date }}
{{ event.location }}
{{ rsvp_url }}
{{ signup_url }}
```

Unknown placeholders currently fail closed.

## QR Codes

Signup QR is available at:

```text
GET /events/{event_ref}/qr/signup.svg
```

Invitee-specific QR generation exists as a backend service primitive, but a
route may still need to be added when the UI needs it.

## Backend Gaps The UI May Need

The current backend intentionally focused on core services and initial route
coverage. UI work may require adding backend-only route handlers for:

- Listing invitees for an event.
- Fetching guests for an invitee in admin mode.
- Updating invitee metadata.
- Regenerating invitee links.
- Rendering/copying message scripts.
- Managing script overrides.
- Returning admin-specific event dashboard aggregates.
- Generating invitee-specific QR SVG.

These should preserve the existing permission model and avoid public data
leaks.

## Verification Baseline

The backend currently passes:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Keep those passing while adding UI-facing route coverage.
