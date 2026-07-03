# Plan: stage_1_secure — hardened fork template

Status: planned. **This plan lives in stage_1 only until the fork exists.**
Sequence: (1) stage_1 lands `2026-07-stage1-hardening-and-http-tests.md`;
(2) stage_1 is forked to `stage_1_secure`; (3) this file moves to the fork
and is **deleted from stage_1** to keep the base template clean.

stage_1_secure is a git fork of stage_1 that periodically `git merge`s from
stage_1 upstream — never a copy. Downstream rule: internal/single-user
tools fork stage_1; anything multi-user or internet-facing forks
stage_1_secure. To keep merges clean, all auth code lives in new modules;
stage_1's existing files are touched in as few, well-marked places as
possible (`app.rs` layer stack + route table, `Cargo.toml`, `_layout.html`
nav).

## Decisions (defaults chosen; revisit before implementation)

These four were proposed and not yet confirmed by the operator — each is a
one-line change to this plan if flipped:

1. **Accounts**: every identity gets an auto-created personal account at
   signup (GitHub model); more accounts can be created and joined later.
   Ownership is never null; single-user forks need zero ceremony.
2. **Factor linking**: link-while-logged-in only. An unauthenticated
   sign-in with an unknown factor creates a new identity; linking a second
   factor requires an authenticated session (Settings → Login methods).
   No auto-link on provider-verified email (account-takeover vector).
3. **Account scoping**: session-pinned active account with a switcher UI,
   not URL-scoped (`/a/{account}/...`). Handlers receive one `AccountScope`.
4. **Authorization granularity**: fixed role enum `owner | admin | member`
   on the membership edge. No permission table until a fork actually needs
   custom roles (migrate the column then).

Confirmed requirements from the operator: factor pluggability is mandatory —
waiting projects need OAuth, OIDC, and QR-code login; identity (entity) ≠
account (unit of legal ownership) ≠ factor (login mechanism).

## The model

- **identity** — the entity that acts: human, agent, or service. The
  attribution target for the audit log. Never hard-deleted (tombstoned via
  `deleted_at`) so audit history keeps its actor.
- **account** — the unit of legal ownership. All domain data FKs to an
  account. Billing, export, and deletion happen at account level.
- **membership** — the edge: `(identity_id, account_id, role)`. Roles live
  here and nowhere else — "is X an admin" is only well-formed per-account.
- **factor** — a login mechanism attached to an identity. Many per
  identity. Proving a factor proves the identity.
- **session** — an authenticated browser/agent context: identity + current
  account.

Implication with the biggest blast radius: **every domain table in every
fork gets `account_id`**, and every store query must be account-scoped.
The template enforces this structurally (see AccountScope below) because
"forgot the WHERE account_id" is the canonical multi-tenant data leak.

## Schema (migrations, one file per table per stage_1 convention)

```
identities   id, kind ('human'|'agent'|'service'), display_name,
             created_at, deleted_at NULL
accounts     id, name, kind ('personal'|'org'), created_at, deleted_at NULL
memberships  identity_id FK, account_id FK, role ('owner'|'admin'|'member'),
             created_at, UNIQUE(identity_id, account_id)
factors      id, identity_id FK, kind, external_id NULL, secret_hash NULL,
             metadata JSON, verified_at NULL, last_used_at NULL, created_at,
             UNIQUE(kind, external_id)
sessions     id, token_hash UNIQUE, identity_id FK, account_id FK,
             created_at, expires_at, last_seen_at, revoked_at NULL,
             user_agent, ip
pending_auth id, kind, token_hash UNIQUE, factor_kind, state JSON,
             identity_id NULL, account_id NULL, expires_at, consumed_at NULL
audit_log    id, at, identity_id NULL, account_id NULL, request_id,
             action, entity, entity_id NULL, detail JSON
invites      id, account_id FK, email, role, token_hash UNIQUE,
             invited_by FK identities, expires_at, accepted_at NULL
```

Notes:

- `factors.external_id` is the provider subject (OIDC `sub`, OAuth user id,
  token id). `UNIQUE(kind, external_id)` — one provider identity maps to
  exactly one of ours. `secret_hash` used by password (argon2) and
  api_token (sha256 of token) kinds; NULL for redirect-based kinds.
- `pending_auth` is the single cross-request state table for **all**
  multi-step flows — OAuth `state` param, magic-link token, QR nonce,
  email-verification token are the same shape: a hashed one-time token,
  a kind, a JSON payload, an expiry. Sweep expired rows opportunistically
  on insert.
- All tokens stored **hashed**; raw values exist only in the cookie/email/
  QR. sqlite PKs declared `NOT NULL` (stage_1 gotcha).
- Audit `identity_id`/`account_id` nullable: pre-auth events (failed
  logins) still get logged.

## Factor pluggability

The plugin axis is a two-phase protocol every mechanism implements
(`src/auth/factor/mod.rs` defines the trait; one module per kind):

```rust
trait FactorKind {
    fn kind(&self) -> &'static str;
    /// Begin auth: returns a Challenge — a redirect URL (OAuth/OIDC),
    /// an emailed token (magic link), a QR nonce to render, or
    /// PromptCredentials (password). May write a pending_auth row.
    async fn start(&self, ctx: StartCtx) -> Result<Challenge>;
    /// Complete auth: consumes the callback/form/poll input plus its
    /// pending_auth row, returns a verified subject
    /// (kind, external_id, claims) — the caller maps it to an identity.
    async fn finish(&self, ctx: FinishCtx) -> Result<VerifiedSubject>;
}
```

A registry (`Vec<Box<dyn FactorKind>>` built in `app.rs` from config) drives
generic routes — `POST /auth/{kind}/start`, `GET|POST /auth/{kind}/finish` —
so adding a factor kind is: one module + one registry line + config. The
mapping from `VerifiedSubject` → identity (existing factor row → log in;
unknown + signup open → create identity/personal account/factor in one
transaction; unknown + authenticated session → link to current identity)
lives **once** in `src/auth/login.rs`, shared by all kinds.

Built-in kinds, phased:

- **password** (phase 1): argon2id via `argon2` crate; degenerate
  single-phase (start = prompt, finish = verify). Standard dummy-hash
  verification on unknown email to keep timing uniform.
- **api_token** (phase 1): for agent identities. Not a session flow — a
  `Bearer` header checked by the auth extractor. Tokens minted from
  Settings (or CLI/seed), shown once, stored hashed. Gets `last_used_at`.
- **oidc** (phase 2): generic OIDC via the `openidconnect` crate — one
  implementation, N providers from config
  (`OIDC_PROVIDERS=google:{issuer,client_id,client_secret},…`). Google/
  Microsoft/Okta are config entries, not code. Plain-OAuth2-only providers
  (e.g. GitHub) get a thin `oauth2`-crate variant reusing the same
  pending_auth flow.
- **magic_link** (phase 2): emailed one-time token. Requires a mail-sender
  boundary: `src/mail.rs` trait with an SMTP impl (`lettre`) and a stage_1-
  convention stub (`warn!("STUB: mail")` + log the link) so forks work
  before SMTP is configured. Doubles as email verification and invite
  delivery.
- **qr_device** (phase 3): device-link login. Unauthenticated device calls
  start → pending_auth nonce → renders QR (client-side JS QR lib in a Solid
  island; no server dep) encoding an approve-URL; an **already
  authenticated** device (e.g. phone) opens it, confirms; original device
  polls `GET /auth/qr/{nonce}/status` (poll, not SSE — simpler, fits the
  existing stack) and gets a session on approval. Nonce TTL ~2 min,
  single-use, shows requester IP/UA on the approve screen.
- **passkey/WebAuthn**: explicit non-goal for now; the factor table shape
  (external_id = credential id, metadata = public key) already fits it.

## Sessions & CSRF

- Hand-rolled minimal session store (`src/store/sessions.rs` + extractor),
  not `tower-sessions` — it's one table + one cookie and matches the
  store-module pattern; fewer deps to drift during upstream merges.
- Cookie: `__Host-session` (fall back to plain name on non-HTTPS local
  dev), `HttpOnly`, `SameSite=Lax`, `Secure` when behind TLS ingress
  (config flag), random 256-bit token, stored hashed. Sliding expiry
  (30d absolute / 7d idle, config). `last_seen_at` updated at most once
  per minute to avoid a write per request.
- Session fixation: token rotated on login and on privilege change
  (account switch keeps identity but re-issues token).
- Logout revokes server-side; Settings lists active sessions with
  revoke buttons (table already has UA/IP).
- **CSRF** — synchronizer token, since `SameSite=Lax` alone doesn't cover
  all POSTs from older clients and subdomain scenarios: per-session token,
  injected into every Askama form via a `csrf_field()` helper on the
  template context, and exposed in a `<meta>` tag that `ts/src/lib/api.ts`
  reads and attaches as `X-CSRF-Token` on mutating fetches. A router
  middleware rejects mutating requests (POST/PUT/PATCH/DELETE) with a
  missing/wrong token — **except** `Bearer`-authenticated API calls (token
  auth is CSRF-immune) and the factor-finish callbacks that arrive from
  provider redirects (protected by the pending_auth one-time token
  instead).

## Extractors & authorization

`src/auth/extract.rs`:

- `CurrentIdentity` — valid session or Bearer token → identity. Rejects
  with 401 (JSON) or redirect-to-login (HTML) based on Accept, reusing the
  error-page middleware.
- `AccountScope` — identity + active account + role, verified against
  `memberships` **on every request** (revocation takes effect immediately,
  not at next login). This is the workhorse: handlers take `AccountScope`,
  and store methods for owned tables take `&AccountScope` (not a raw
  account_id), so an unscoped query is a compile-time-visible smell.
  Exemplar: convert the demo guestbook to account-owned in the fork —
  forks then copy a scoped example, not an unscoped one.
- `RequireRole<const R: Role>` (or a method `scope.require(Role::Admin)?`)
  — prefer the method form; const-generic extractors are cleverness the
  template doesn't need.

## Audit log

`src/store/audit.rs`: `audit(&scope_or_actor, action, entity, entity_id,
detail_json)` — one call per mutation, wired into every exemplar mutation
so forks copy the habit. Records `request_id` from the tracing span (joins
audit rows to logs). Pre-auth events (`login.failed`, `login.succeeded`,
`factor.linked`, `session.revoked`, `invite.accepted`, role changes) are
logged by the auth module itself. Read UI: a simple `/account/audit` page,
admin+, filterable by entity — table + pagination, no island needed.

## Routes & pages

```
GET  /login /signup           factor picker (from registry) + forms
POST /auth/{kind}/start       generic factor start
GET|POST /auth/{kind}/finish  generic factor finish (callback/form/poll)
POST /logout
GET  /settings                profile, factors (add/remove — cannot remove
                              last factor), api tokens, active sessions
GET  /account                 account settings, rename, danger zone
GET  /account/members         list, roles, invites (admin+)
POST /account/switch          set session's active account
GET  /account/audit           audit log (admin+)
GET  /invite/{token}          accept invite (login/signup interstitial)
```

Signup policy: `AUTH_SIGNUP=open|invite|closed` (default `open`; deployment
docs say set `invite` or `closed` in production). Invites carry a role and
create a membership on accept; delivered via the mail boundary (stub logs
the link until SMTP is configured).

Signup transaction (one place, `src/auth/login.rs`): identity + factor +
personal account + owner membership, atomically.

## Config additions

`SESSION_TTL_*`, `COOKIE_SECURE`, `AUTH_SIGNUP`, `OIDC_PROVIDERS`,
`SMTP_*`, `BASE_URL` (needed for OAuth callbacks / magic links / QR
approve-URLs — first config that makes the server's public URL explicit).
All via `Config` per stage_1 convention.

## New dependencies

`argon2`, `rand`, `axum-extra` (cookie feature), `time` (phase 1);
`openidconnect` or `oauth2`, `lettre` (phase 2); client-side JS QR encoder
(phase 3). Each phase-gated dep lands with its phase, not up front.

## Testing

Extends the stage_1 router-test harness (`test_util::test_app`), same
<~1s in-memory budget. Auth exemplar tests:

1. Full signup → session cookie → authed page roundtrip.
2. Login wrong password → 401 + `login.failed` audit row.
3. Mutating POST without CSRF token → 403; with token → 2xx.
4. **Cross-account isolation**: identity A creates data in account 1;
   identity B (no membership) requests it → 404/403. The single most
   important test in the template — it guards the AccountScope pattern.
5. Role gating: member hits admin route → 403.
6. Revoked session → redirected to login.
7. Bearer token: agent identity hits JSON API without cookie/CSRF → 200;
   revoked token → 401.
8. Factor lifecycle: link second factor while authed; removing last
   factor → 4xx.
9. pending_auth one-time-use: replayed magic-link/OAuth-state token → 4xx.

Playwright: the login → switch account → mutate → audit flow is stage_1's
stated "real multi-step user flow" threshold — add it in this fork
(phase 3), per AGENTS.md.

## Phasing

- **Phase 1 — core**: schema, sessions, CSRF, password + api_token
  factors, extractors, AccountScope + scoped-guestbook exemplar, audit
  log, signup/login/logout/settings pages, tests 1–8. The fork is usable
  by downstream projects at the end of phase 1.
- **Phase 2 — federated + email**: mail boundary, magic_link, generic
  OIDC/OAuth, invites + member management, test 9.
- **Phase 3 — device + polish**: qr_device flow, active-session UI,
  audit page, playwright flow.

## Merge discipline (fork hygiene)

- `git remote add upstream <stage_1>`; merge upstream on a cadence.
- New code in `src/auth/`, `src/store/{identities,accounts,memberships,
  factors,sessions,pending_auth,audit,invites}.rs`, `src/mail.rs`,
  `templates/auth/`, `ts/src/islands/{qr-login,...}` — files stage_1 will
  never touch.
- stage_1 files modified: `app.rs` (one marked block in the route table +
  layer stack), `config.rs` (appended fields), `Cargo.toml`, `_layout.html`
  (nav auth widget), AGENTS.md/README (fork-specific sections appended).
- After forking: delete `docs/plans/2026-07-secure-fork-template.md` from
  stage_1; this file becomes the fork's implementation plan.
