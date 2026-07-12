# Design: Circles, Guest Accounts, Viewer Resolution, Photos, Calendar

Explored: `stage_2` migrations 0001–0010 + `AGENTS.md`, `src/auth/{extract,middleware}.rs`, `src/config.rs`; events-fork migrations 0011–0021, `src/app.rs` (gather/admin bin split), `src/handlers/event_public.rs`, `src/store/{events,event_links,people,accounts}.rs`, `src/bin/admin.rs`. Key facts that shape everything below: the events fork already has the two-bin split (`gather` = public/no-auth, `admin` = mesh-only/`AccountScope`); tier enforcement today is exactly 2 chokepoints (`Event::public_view`, `Store::list_schedule`'s tier filter); `Store::require_single_account` hard-fails on >1 account row, which the guest-account decision below has to reconcile with.

---

## Key decisions up front

1. **Guest identities never get a membership in the owner's account.** Each claimed guest gets their own throwaway personal `account` (satisfies stage_2's "every identity has exactly one owning account" invariant), but all guest-surface reads resolve the **owner's** `account_id` via a new join table and filter by `(owner_account_id, person_id)` — never by the guest's own account. Justification and the `require_single_account` fallout are in §g.
2. **Circle/person visibility is a grant list, not a 3-way enum.** `audience_policies` (1 per event/calendar_entry) + `audience_circle_grants` + `audience_person_overrides` — public/circles/people falls out of which grant tables have rows, and each grant carries its own render `level`.
3. **Event-linked calendar entries are a read-time union**, not mirrored rows.
4. **Photos are content-hash addressed**, EXIF stripped unconditionally (this app's whole premise is address privacy).
5. The "tier enforced in exactly 2 places" property is generalized to **exactly 1 redaction chokepoint per subject type** (events, schedule_items, calendar_entries, photos), fed by one pure `level_for()` function — see §g.

---

## (a) Migration list

Assumes this repo carries events' `0011`–`0021` verbatim as its own (identical base through `0010`). New migrations start at `0022`.

```
0022_circles.up.sql
CREATE TABLE circles (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, name)
);
CREATE INDEX idx_circles_account ON circles(account_id);

0023_circle_members.up.sql
CREATE TABLE circle_members (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    circle_id INTEGER NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(circle_id, person_id)
);
CREATE INDEX idx_circle_members_person ON circle_members(person_id);

0024_audience_policies.up.sql
-- One row per (event | calendar_entry). Owns the "public" grant; circle/person
-- grants live in the two tables below and reference this row, not the subject
-- directly, so both subject types share one redaction contract.
CREATE TABLE audience_policies (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    subject_type TEXT NOT NULL CHECK (subject_type IN ('event', 'calendar_entry')),
    subject_id INTEGER NOT NULL,
    public_level TEXT NOT NULL DEFAULT 'hidden'
        CHECK (public_level IN ('hidden', 'busy', 'summary', 'full')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(subject_type, subject_id)
);

0025_audience_circle_grants.up.sql
CREATE TABLE audience_circle_grants (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    policy_id INTEGER NOT NULL REFERENCES audience_policies(id) ON DELETE CASCADE,
    circle_id INTEGER NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    level TEXT NOT NULL CHECK (level IN ('hidden', 'busy', 'summary', 'full')),
    UNIQUE(policy_id, circle_id)
);

0026_audience_person_overrides.up.sql
-- level is NULL iff override_kind='exclude'; required iff 'include'.
CREATE TABLE audience_person_overrides (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    policy_id INTEGER NOT NULL REFERENCES audience_policies(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    override_kind TEXT NOT NULL CHECK (override_kind IN ('include', 'exclude')),
    level TEXT CHECK (level IN ('hidden', 'busy', 'summary', 'full')),
    UNIQUE(policy_id, person_id)
);

0027_person_identity_links.up.sql
-- Separate join table, not a column on people (see §g #2): keeps a history
-- of claim/unlink cycles and mirrors the memberships-as-edge-table pattern.
CREATE TABLE person_identity_links (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    identity_id INTEGER NOT NULL REFERENCES identities(id),
    claimed_at TEXT NOT NULL DEFAULT (datetime('now')),
    unlinked_at TEXT
);
CREATE UNIQUE INDEX idx_pil_active_person ON person_identity_links(person_id) WHERE unlinked_at IS NULL;
CREATE UNIQUE INDEX idx_pil_active_identity ON person_identity_links(identity_id) WHERE unlinked_at IS NULL;

0028_people_recovery_email.up.sql
-- Separate from the freeform `contact` field: this one is machine-used
-- (password-reset delivery), so it needs its own unambiguous column.
ALTER TABLE people ADD COLUMN recovery_email TEXT;

0029_calendar_entries.up.sql
CREATE TABLE calendar_entries (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    title TEXT NOT NULL,
    location TEXT NOT NULL DEFAULT '',
    starts_at TEXT NOT NULL,
    ends_at TEXT,
    timezone TEXT NOT NULL DEFAULT 'America/Los_Angeles',
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_calendar_entries_account ON calendar_entries(account_id, starts_at);

0030_calendar_feed_tokens.up.sql
-- token_plain kept, same rationale as event_links: admin re-copies/re-QRs it.
CREATE TABLE calendar_feed_tokens (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    token_plain TEXT NOT NULL,
    revoked_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, person_id)
);

0031_photos.up.sql
CREATE TABLE photos (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    uploaded_by_identity_id INTEGER REFERENCES identities(id),
    uploaded_by_person_id INTEGER REFERENCES people(id),
    storage_key TEXT NOT NULL,
    original_filename TEXT NOT NULL DEFAULT '',
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    caption TEXT NOT NULL DEFAULT '',
    taken_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT
);
CREATE INDEX idx_photos_event ON photos(event_id, deleted_at);
CREATE INDEX idx_photos_storage_key ON photos(storage_key);

0032_photo_variants.up.sql
CREATE TABLE photo_variants (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('original', 'thumb', 'medium')),
    storage_key TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    byte_size INTEGER NOT NULL,
    UNIQUE(photo_id, kind)
);

0033_accounts_purpose.up.sql
-- See §g: guest accounts break require_single_account's "exactly one
-- account" assumption. Make the owner account explicit instead of implicit.
ALTER TABLE accounts ADD COLUMN purpose TEXT NOT NULL DEFAULT 'primary'
    CHECK (purpose IN ('primary', 'guest'));
```

Not a migration: `Store::create_event`/`create_calendar_entry` must insert a matching `audience_policies` row (`public_level='hidden'`) in the same transaction as the subject row — there is no DB trigger for this by convention (query macros only, per `AGENTS.md`), so it's an application-level invariant, tested at the store layer.

---

## (b) Viewer resolution + visibility computation (pseudocode)

```
enum Level { Hidden = 0, Busy = 1, Summary = 2, Full = 3 }   // Ord derived

enum Viewer {
    Anonymous,
    LinkHolder { person_id: Option<PersonId>, event_id: EventId },   // from /e/{token}
    Guest { identity_id, person_id: PersonId },                     // claimed, session-based
    Owner { identity_id },                                          // session.account_id == owner_account_id
}

// --- resolution: one extractor (session half) + the existing token resolver ---
fn resolve_viewer(session_ctx: Option<SessionContext>, link: Option<ResolvedLink>) -> (Viewer, Option<MismatchNote>):
    owner_account_id = app_state.owner_account_id   // cached at startup via require_primary_account()

    session_viewer =
        match session_ctx:
            Some(ctx) if ctx.account_id == owner_account_id -> Some(Owner{identity_id: ctx.identity_id})
            Some(ctx) -> match find_active_person_link(ctx.identity_id):
                             Some(person_id) -> Some(Guest{ctx.identity_id, person_id})
                             None            -> None   // orphaned/unclaimed identity -> treat as anonymous
            None -> None

    token_viewer = link.map(|l| LinkHolder{person_id: l.person_id, event_id: l.event_id})

    match (session_viewer, token_viewer):
        (Some(Owner{..}), _)                                    -> (Owner, None)
        (Some(Guest{p, ..}), Some(LinkHolder{Some(lp), ..})) if p == lp
                                                                  -> (Guest{p}, None)          // token superfluous
        (Some(Guest{..} as g), Some(LinkHolder{..} as l))       -> (l, Some(MismatchNote{g}))  // content follows token
        (Some(g @ Guest{..}), None)                              -> (g, None)
        (None, Some(l))                                          -> (l, None)
        (None, None)                                             -> (Anonymous, None)

// --- pure, I/O-free: the ONE place level math happens ---
fn level_for(viewer: &Viewer, policy: &AudiencePolicy,
             overrides: &[PersonOverride], circle_grants: &[CircleGrant],
             person_circles: &[CircleId]) -> Level:
    if let Owner{..} = viewer: return Full

    person_id = match viewer:
        Guest{person_id, ..} | LinkHolder{person_id: Some(person_id), ..} -> Some(person_id)
        _ -> None

    if let Some(pid) = person_id:
        if let Some(o) = overrides.find(|o| o.person_id == pid):
            return match o.override_kind: Exclude -> Hidden, Include -> o.level  // wins over circles & public

        circle_level = circle_grants
            .filter(|g| person_circles.contains(g.circle_id))
            .map(|g| g.level)
            .max()                          // most-generous circle wins — see §f
            .unwrap_or(Hidden)
        return max(circle_level, policy.public_level)

    // anonymous, or shared (person-less) link
    return policy.public_level

// --- direct single-event page floor (see §f) ---
fn level_for_direct_hit(viewer, link: &ResolvedLink, policy) -> Level:
    level = level_for(viewer, policy, ...)
    if level == Hidden: return Hidden                 // explicit exclude, or genuinely not invited -> 404
    if level == Busy:   return Summary                 // holding a resolvable token is itself proof of invite
    return level
```

**Architectural placement** (answers "where does it live, keeping enforcement in few places"):
- Session-half resolution: a new `Viewer` `FromRequestParts<AppState>` extractor in `src/auth/viewer.rs`, gather-bin only, parallel to `AccountScope`/`NavContext`. It cannot see path params generically, so token resolution stays exactly where it already lives (`event_public::resolve()`); handlers combine `Viewer` + `Option<ResolvedLink>` via `Viewer::combine_with_link(...)`.
- `level_for` / `level_for_direct_hit`: one pure module, `src/access/level.rs`, zero I/O, unit-testable without a DB.
- Redaction chokepoints, one per subject type, each calling `level_for` and nothing else deriving policy: `Event::view_for(level)` (renamed/generalized `public_view`), `Store::list_schedule(account_id, event_id, level)` (tier filter generalized to level), `CalendarEntry::view_for(level)` (new), `Store::list_photos_for_viewer(...)` (new, gated by attendance not level — see §e). Handlers never re-derive redaction; they call one of these four.

---

## (c) Routes by bin

**Gather bin** (public, internet-facing; adds `attach_session` — new for this bin, see §g):
- existing: `/`, `GET /e/{token}`, `GET/POST /api/e/{token}[/rsvp]`, `/healthz`, `/api/client-errors`, `/static`
- `GET/POST /e/{token}/claim` — claim form/submit (rate-limited, CSRF once session exists)
- `GET/POST /login`, `POST /logout` — guest login, distinct template/handler from admin's `/login`
- `GET /my` — claimed-guest dashboard (session-scoped, via `GuestScope`)
- `GET /my/events/{event_id}` — session-scoped equivalent of `/e/{token}`
- `GET /calendar`, `GET /calendar/{token}.ics`
- `GET /e/{token}/photos/{photo_id}/{variant}`, `GET /my/events/{event_id}/photos/{photo_id}/{variant}` — authz-checked serving
- `POST /e/{token}/photos`, `POST /my/events/{event_id}/photos` — own body-size layer
- `POST /e/{token}/photos/{photo_id}/delete` + session equivalent — uploader-only

**Admin bin** (mesh-only, `AccountScope`-gated):
- existing: `/events*`, `/people*`, `/login`, `/signup`, `/logout`, `/settings*`, `/account*`
- `GET/POST /circles`, `GET/POST /circles/{id}`, `POST/DELETE /circles/{id}/members`
- `GET/POST /events/{event_id}/audience` — edit public_level + circle grants + person overrides
- `GET /calendar`, `POST /calendar/entries[/{id}]` — standalone entries, same audience editor
- `POST /people/{person_id}/calendar-feed[/revoke]`
- `GET /events/{event_id}/photos`, `POST .../photos` (owner upload), `POST .../photos/{id}/delete` (admin, any)
- `GET/POST /people/{person_id}/claim-status` — recovery: view/force-unlink a claimed identity
- CLI (`admin` bin, no HTTP): `import-legacy-db <path>`, `verify-import`, `photos-gc --older-than <days>`, `mint-calendar-feed <person>`, `set-audience <slug> --public <level> | --circle <name>=<level> | --person <name>=<include:level|exclude>`

---

## (d) Claim-flow state diagram

```
[person-bound event_link, unrevoked, not yet linked]
        |
        |  GET /e/{token}  -> "claim your account" CTA shown
        v
  [claim form: password, confirm, optional email]
        |
        |  POST /e/{token}/claim  (validated, rate-limited)
        v
  txn: create identity(kind=human)
     -> create account(purpose='guest', kind='personal')
     -> create membership(identity, guest_account, role=owner)   // satisfies stage_2 invariant
     -> create factor(kind='password', external_id=synthetic 'guest:{person_id}',
                       secret_hash=argon2(password))
     -> if email given: people.recovery_email = email  (NOT external_id)
     -> create person_identity_links(person_id, identity_id)
     -> create session, set cookie
        |
        v
  [CLAIMED] -- identity now has: own throwaway account (unused for domain data),
              a password factor, and a live person_identity_links row --

  from CLAIMED, three independent axes can move without affecting the others:

  (1) admin revokes the ORIGINAL token used to claim
        -> that /e/{token} 404s (unknown-vs-revoked, same as today)
        -> CLAIMED identity's password login is UNAFFECTED (different credential)

  (2) admin re-mints the person's link (revoke + new event_links row)
        -> new /e/{token'} works; old one 404s
        -> CLAIMED identity's password login is UNAFFECTED
        -> link is now a "convenience shortcut", not the sole access path

  (3) admin/support force-unlinks (person_identity_links.unlinked_at = now)
        -> back to [person-bound event_link, not yet linked] state
        -> a NEW claim (new identity+account+factor) can occur later;
           old identity/factor/guest-account are tombstoned, not deleted
           (audit_log keeps its actor)
```

---

## (e) Photo storage + serving

**Layout** (bind-mounted volume, sibling to `data/app.db`):
```
data/photos/{account_id}/{event_id}/{sha256_hex}.{ext}              # original (post EXIF-strip)
data/photos/{account_id}/{event_id}/{sha256_hex}.thumb.webp         # 320px
data/photos/{account_id}/{event_id}/{sha256_hex}.medium.webp        # 1280px
```
Content-hash over UUID: free de-dup when two guests upload the same shot (second insert reuses the existing `storage_key`, skips the write), the filename doubles as an integrity check, and immutable content-addressed paths are cache-friendly (`Cache-Control: private, immutable`). No refcounted CAS system — dedup is a cheap side effect (`SELECT count(*) FROM photos WHERE storage_key = ?` before unlinking on delete), not a subsystem.

**Ingest pipeline** (upload handler, synchronous — personal-site scale, no queue):
1. Sniff magic bytes (not client `Content-Type`) against an allowlist: jpeg/png/webp/heic.
2. Decode via `image` crate; read EXIF `DateTimeOriginal`/GPS *before* stripping → `taken_at` column; then re-encode without EXIF for **every** stored variant, including "original" — GPS-in-photo would defeat the address-tier privacy model this app is built around.
3. Generate `thumb`/`medium` variants inline.
4. Hash the stripped original; write files (skip if `storage_key` already on disk); insert `photos` + `photo_variants` rows in one transaction.

**Access control**: "attendee" = `attendance.status IN ('going', 'attended')` for that event, or Owner. Rejected alternative: "anyone with an invite" (level ≥ Summary) — too loose, since an unconfirmed/never-responded invitee shouldn't see photos before deciding to attend.

**Serving**: never `ServeDir`/`/static` (no per-file authz hook). A dedicated route per bin (`/e/{token}/photos/{id}/{variant}`, `/my/events/{event_id}/photos/{id}/{variant}`) resolves the viewer, checks the attendee predicate, then streams via `tokio::fs::File` + explicit headers. Deletion: uploader soft-deletes own (`deleted_at`), admin soft/hard-deletes any; listings exclude `deleted_at IS NOT NULL` immediately everywhere; disk GC is a separate `admin photos-gc` sweep, not eager, to keep the upload/delete handlers simple.

**Body size**: photo routes get their **own** `RequestBodyLimitLayer` (recommend 15 MiB) nested in a sub-`Router` merged *before* the outer 1 MiB global layer applies — first per-route override in either app (§g #4).

---

## (f) Edge-case decisions

- Token/session mismatch on `/e/{token}`: content follows the **token**; a banner surfaces the session identity; no auto-merge, no auto-logout, writes attribute to whichever identity actually authenticated the write (token-anonymous RSVP vs session-authenticated).
- `exclude` override beats circle/public grants, but never beats Owner.
- Person in multiple circles: **max** (most generous) level wins, not min — a "closest friends" + "coworkers" overlap should get the friends-level access, not be capped down.
- A resolvable, unrevoked token always floors at `Summary` on the direct event page (holding it is evidence of invitation) even if computed level is `Busy`; an explicit `exclude` still forces `Hidden`.
- Revoking a link post-claim does not revoke the claimed password login (different credentials); re-minting a link post-claim is cosmetic/backup only.
- Claim always requires a password; email is optional and never doubles as the login key (`external_id` is a synthetic `guest:{person_id}` handle) — avoids `UNIQUE(kind, external_id)` collisions for two guests sharing an email or giving none.
- New events/calendar entries default `public_level='hidden'` (opt-in visibility), matching current behavior where nothing renders without a token.
- Photo "attendee" is RSVP-status-based, not invite-based (see §e).
- Legacy `sessions` rows are dropped, not migrated — force fresh login after the schema cutover rather than trusting stale expiry math across a host/schema change.
- Legacy `identities`/`accounts`/`people`/`events`/`event_links` primary keys are preserved verbatim on import (no ID remapping), specifically because `resolve_event_link` looks up only by `token_hash` — this is what makes `/e/{token}` continuity a structural guarantee rather than a best-effort mapping table. Import must run before the operator's own `/signup`, into an otherwise-empty freshly migrated DB, to avoid PK collision with a locally-bootstrapped owner identity.
- Per-person-bound legacy `event_links.tier='private'` rows backfill as `audience_person_overrides(include, level=full)`; `tier='public'` rows backfill the event's `audience_policies.public_level='summary'` if any public link exists for that event, else stays `hidden`.

---

## (g) Divergences from stage_2 conventions

1. **Gather bin gains `attach_session`** — today it has none by design ("guests have no account and never log in"). This is a real behavior change to that bin; every new mutating gather route (claim, session-authenticated RSVP/photo upload) must get the same `csrf::verify` discipline the admin bin already has, whereas today gather-bin mutations rely only on rate-limiting/anonymity.
2. **A new `GuestScope` extractor resolves an `account_id` that is *not* the caller's own membership account** — it walks `person_identity_links` to the **owner's** account. This is a deliberate, narrow exception to "AccountScope is how you get an account_id"; it's scoped to guest routes only, and every existing admin route is untouched.
3. **`require_single_account` breaks** once guest accounts exist (it hard-fails on >1 row). Fixed by adding `accounts.purpose` (`'primary'`/`'guest'`, migration `0033`) and changing the CLI/startup lookup to filter `purpose='primary'` — renamed conceptually to "the platform owner account." Guest accounts are `purpose='guest'`, created only by the claim flow, never touched by admin CLI tooling that assumes a single owner.
4. **"Exactly 2 places" generalizes to "1 chokepoint per subject type"** (4 total: events, schedule_items, calendar_entries, photos) — the count grows because subject types grow, but the structural property (one pure `level_for`, never re-derived in a handler) is what's actually preserved.
5. **First per-route body-size override** — photo upload nests its own `RequestBodyLimitLayer` inside the outer global-1MiB-layered router. `AGENTS.md` already anticipates this ("raise it per-handler later").
6. `event_links.tier` is superseded by `audience_policies`/`audience_person_overrides` as the actual rendering authority; the column stays (read only during import backfill) rather than being dropped, to avoid a breaking schema change with no immediate functional payoff — flagged as a follow-up cleanup migration.
7. `people.recovery_email` is a new, separate column from the existing freeform `people.contact` — different trust level (machine-used for password reset vs. admin-typed notes), so it doesn't overload an existing field's meaning.

---

### Critical Files for Implementation

- `C:/Users/ronit/dev/stage_2/src/auth/extract.rs` — where `Viewer`/`GuestScope` extractors get added alongside `AccountScope`/`NavContext`
- `C:/Users/ronit/dev/stage_2/src/auth/middleware.rs` — `attach_session` needs to be layered onto the gather bin (currently admin-bin-only pattern)
- `C:/Users/ronit/dev/personal/events/src/handlers/event_public.rs` — the existing `resolve()`/`build_view()` chokepoint to generalize from tier-bool to `Level`, and where claim/mismatch handling attaches
- `C:/Users/ronit/dev/personal/events/src/store/events.rs` and `src/store/event_links.rs` — `Event::public_view`/`resolve_event_link`, the exact functions being generalized and reused by the import/backfill logic
- `C:/Users/ronit/dev/personal/events/src/app.rs` — `build_gather_router`/`build_admin_router`/`apply_layers`, where new routes, the per-route photo body-size layer, and the gather-bin session middleware all get wired in
- `C:/Users/ronit/dev/personal/events/src/bin/admin.rs` — CLI dispatch pattern to extend with `import-legacy-db`, `verify-import`, `photos-gc`, `set-audience`
- `C:/Users/ronit/dev/stage_2/src/config.rs` — where `PHOTO_MAX_BODY_BYTES` and any new env tunables get added alongside `max_body_bytes`
