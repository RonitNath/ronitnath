# The OpenID Provider (leg O1, 2026-08-27)

This deployment is an OpenID Provider. It has its own issuer, its own signing
keys and its own client registry, and **nothing federates to another
deployment**: `ronitnath.com` and `isoastra.com` are two providers that have
never heard of each other. That is the point rather than a limitation — "sign
in with your account" means one account here, not an implicit trust
relationship nobody wrote down.

The Provider is a feature of the **platform crates**, not of this site. A
downstream deployment — a client project depending on `rn-kernel` and `rn-site`
as git dependencies — mounts one router, sets three variables and is an OpenID
Provider. Nothing in `crates/kernel/src/oidc/**` or `crates/server/src/oidc/**`
names this deployment: the issuer, the deployment's display name, the sealing
key, the registry and the sector a client's subjects are pairwise under all
come from configuration or from the store. See §What a downstream deployment
needs.

---

## The three rulings

**A token dies with its session.** `oidc_token.session_id` is `NOT NULL` for a
user token — the schema's `CHECK` says so — and every path that ends a session
takes its tokens and its unredeemed codes with it, in the same transaction.
There is no sweeper to fall behind and no relying party that keeps working
after somebody signs out. Access tokens are therefore **opaque and hashed at
rest**, not JWTs: a self-contained access token is a token nobody can revoke,
because the resource server verifies a signature and asks nothing.

**`sub` is pairwise by *sector*, and the sector is the client's owner.** Two
applications of one organization recognise the same person; two organizations
cannot correlate their users at all, however many clients each registers. For a
client the operator registered with no owner, the sector is the deployment
itself — `platform:*`, carried as `0` in `oidc_subject.sector_party_id`, the
same shape `relation` already uses for `public` and `authenticated`.

A `sub` is minted once, never rotated, and is **never an id**. A public id is a
reversible encryption of a rowid under this deployment's key; a `sub` is 256
random bits that name nothing. After a merge the absorbed person's row stays
and *both* subs still resolve, because a `sub` names a person and a person
resolves through `person_alias`.

**Consent is a relation.** `oidc_client:X #authorized @person:Y` goes through
the same `check()` as every other authorisation in the system, so there is no
second authorisation path to disagree with the first. The scopes agreed to are
the attribute a relation row cannot carry, so they are `oidc_consent`, read
only after `check()` has said yes.

---

## The handle

A person's handle is their `preferred_username`: human-chosen, unique
deployment-wide, lowercase `[a-z0-9-]{3,32}`, chosen at registration (a form
field, prefilled from the address's local part) and changed by `set-handle`.

It is not an id, and it never stops resolving. A handle a person gives up is
filed in `party_handle_alias` against that same person, which keeps it
resolving and keeps it unavailable to anybody else — and because the alias
names *them*, they may take it back. A merge does the same: the survivor keeps
its handle and the absorbed one becomes an alias.

---

## What is implemented

| | |
|---|---|
| **Discovery** | `GET /.well-known/openid-configuration` and `/.well-known/oauth-authorization-server` (RFC 8414) — the same document, because they describe the same server. |
| **Authorization Code** | `response_type=code`, `response_mode=query`. PKCE **S256 required of every client type** (RFC 9700 §2.1.1); `plain` is refused. |
| **PKCE** | RFC 7636, S256 only. A verifier is checked for its own shape (43–128 unreserved characters) before it is compared. |
| **Refresh** | Rotation, with **family revocation on reuse** (RFC 9700 §4.14.2). Minted only when `offline_access` was consented to. |
| **Client credentials** | Mints a token whose principal is the client's own `service` party — the first route in the deployment that accepts that party kind. Confidential clients only. |
| **`prompt`** | `none`, `login`, `consent`, `select_account`, with `login_required`, `consent_required`, `interaction_required`, `account_selection_required`. `prompt=none` never renders anything. |
| **`max_age` / `auth_time`** | `auth_time` is when the session was minted, which is when it last proved a factor. A session older than `max_age` is sent to re-authenticate. |
| **`id_token_hint`** | Validated as ours — signature under a published key, and the issuer — and **not** checked for expiry, which is what the specifications ask of a hint. |
| **`login_hint`** | Carried to `/auth` as the address to prefill. |
| **RFC 9207** | `iss` on every authorization response, success and failure alike; `authorization_response_iss_parameter_supported` is `true`. |
| **UserInfo** | `GET` and `POST`, bearer only. `sub` always; `name`/`preferred_username`/`updated_at` under `profile`; `email`/`email_verified` under `email`; `groups` under `groups`, scoped to the client owner's subtree and nothing outside it. |
| **JWKS** | `GET /oidc/jwks`, `use: sig`, `alg: RS256`, cacheable for five minutes. Publishes the active key **and the retiring ones**, which is what makes rotation a non-event. |
| **Revocation** | RFC 7009. Client-authenticated; `200` for a token this deployment never minted; a refresh token takes its family. |
| **RP-Initiated Logout** | `GET|POST /oidc/end_session`. `id_token_hint`, `client_id`, `post_logout_redirect_uri` (exact match, and only when a hint identified the client), `state`. A request with no hint — or one naming another session — is **confirmed with a page and a button** rather than obeyed. |
| **Back-Channel Logout** | A `logout_token` (`typ: logout+jwt`, `iss aud iat exp jti events` and `sid`, or `sub` for a bulk disable; **never `nonce`**) POSTed to each client that held a token under the session and registered an endpoint. Off the request path, one attempt and one retry. |
| **Client authentication** | `client_secret_basic`, `client_secret_post`, `private_key_jwt` (RFC 7523, with `jti` replay refused by a primary key), `none`. The method is the *registration's*, never the request's. |
| **Rate limiting** | Failed client authentications, per client, per node: 20 in 60 seconds, cleared by a success. |

### Declared unsupported

Each with the specification's own word for it, and refused rather than ignored
— a server that silently drops a parameter answers a question nobody asked.

| declared unsupported | how it is refused |
|---|---|
| `request` (request objects) | `request_not_supported`, metadata `request_parameter_supported: false` |
| `request_uri` | `request_uri_not_supported`, metadata `request_uri_parameter_supported: false` |
| `claims` request parameter | metadata `claims_parameter_supported: false`; the parameter is not read |
| Implicit and Hybrid flows | `unsupported_response_type`; `response_types_supported: ["code"]` |
| `response_mode=form_post` | `response_modes_supported: ["query"]` |
| `code_challenge_method=plain` | `invalid_request`; `code_challenge_methods_supported: ["S256"]` |
| Encrypted id tokens / JWE | not offered: no `id_token_encryption_*` metadata |
| JWT access tokens (RFC 9068) | access tokens are opaque, by the first ruling above |
| Dynamic registration (RFC 7591) | no endpoint. The *row* is RFC 7591's shape, so adding one is an endpoint and an authorisation rule, not a migration |
| WebFinger / issuer discovery (Core §2 of Discovery) | no `/.well-known/webfinger`: this deployment is its own issuer and there is nothing to discover from an address |
| Device Authorization Grant (RFC 8628) | no `device_authorization_endpoint` |
| `client_secret_jwt` | not in `token_endpoint_auth_methods_supported` — an HMAC over a secret buys nothing `client_secret_basic` does not |
| Token introspection (RFC 7662) | no endpoint; `/oidc/userinfo` answers the only question a resource server here has |
| Front-channel logout | no `frontchannel_logout_uri`: back-channel is the one that works with third-party cookies blocked |
| Pairwise `sector_identifier_uri` | the sector is the client's *owner*, from the registry, not a document the client hosts |

### Known gaps

* **An `https` back-channel logout endpoint is not delivered to.** There is no
  TLS client in this workspace, and adding one for a single form POST is a
  dependency for one request shape. `http` endpoints (loopback, and a mesh) are
  delivered to; an `https` one is logged and skipped. The authoritative answer
  is unaffected: the token stopped working in the transaction that ended the
  session.
* **`oidc_consent` does not move on a merge.** The *grant* does — it is a
  relation row, and merge unions those onto the survivor — but the scope
  attribute beside it stays with the absorbed person, so the survivor is asked
  to consent again. That is the safe direction, and it is asserted in
  `crates/kernel/tests/merge_properties.rs`.
* **Expiry sweeps for `oidc_code`, `oidc_token` and `oidc_assertion` are
  written (`SWEEP_SQL`) and not yet on the observation lane.** Nothing depends
  on them for correctness — every read checks `expires_at` — so this is disk,
  not latency.

---

## Key material and rotation

RS256, 2048-bit RSA, one algorithm. Keys live in three statuses:

* `active` — what new tokens are signed under. Exactly one; the rotation
  statement moves the previous one out of the way *before* the insert, in the
  same transaction, so there is never a moment with two.
* `retiring` — no longer signs, still verifies, still in the JWKS. This is the
  overlap that makes rotation a non-event for an RP with a cached document.
* `retired` — gone from the JWKS.

The private half is sealed at rest with AES-256-GCM under `RN_SITE__OIDC_KEY`
(or the file `RN_SITE__OIDC_KEY_FILE` names). **Not the id key**, and
deliberately a second variable: the id key names rows, and this one forges
identity — one secret that did both would be one leak that did both.

Rotate with `POST /api/cmd/rotate-signing-key` as a platform operator. Retiring
a key for good is a second, later decision, taken when nothing signed under it
is still alive.

---

## What a downstream deployment needs

1. **Configuration** — three keys, and one optional:

   | key | required | what it is |
   |---|---|---|
   | `RN_SITE__PUBLIC_ORIGIN` | prod (https enforced) | the issuer, **exactly**: scheme, host, port, no trailing slash. Every endpoint URL in the discovery document is built from it, and every RP compares it byte for byte. Defaults to `http://127.0.0.1:3004` in dev. |
   | `RN_SITE__OIDC_KEY` / `_FILE` | prod | 64 lowercase hex characters (AES-256). Seals the signing keys' private halves. A dev process mints an ephemeral one and warns; keys already in the database will not open under it. |
   | `RN_SITE__ID_KEY` / `_FILE` | prod (already) | unchanged — public-id derivation. |
   | `RN_SITE__PUBLIC_NAME` | no | what the consent and sign-out pages call the deployment. Absent, the issuer's host. |

2. **The migration** — `crates/kernel/migrations/6_oidc.sql` ships with the
   kernel crate and is applied by `Client::migrate::<rn_kernel::Migrations>`
   like every other. There is nothing deployment-specific in it.

3. **The routes** — `rn_site::oidc::router()` returns a `Router<AppState>`
   carrying all eight paths (`rn_kernel::oidc::paths`). Merge it as a peer of
   the other feature routers. It reaches into nothing outside
   `crates/server/src/oidc/**`, `crates/kernel/src/oidc/**` and the shared
   template base.

4. **The templates** — `templates/consent.html`, `templates/end_session.html`
   and `templates/oidc_error.html` extend the same `base.html` as `/auth` and
   are styled by tokens alone. A deployment with its own template set supplies
   its own three with the same field names.

5. **The platform organization** — there is none to create. A client registered
   with no `owner` belongs to the deployment itself (`platform:*`), and its
   subjects are pairwise under sector `0`. A client owned by an *organization*
   names that organization's public id as `owner`, and the sector is that
   organization's party row.

6. **Registering a client** — `POST /api/cmd/register-client` as a platform
   operator (`platform:* #operator @person`), with the RFC 7591-shaped body.
   The reply's event carries the client's public id, which **is** its
   `client_id`. The secret is in the command's own return value and is
   therefore only readable by a caller that runs the kernel command directly —
   `tools/oidc-rp.sh` does exactly that. `rotate-client-secret` mints a new one
   and kills the old in the same statement.

---

## The RP setup recipe (`oauth2-proxy`)

`tools/oidc-rp.sh` registers a client, prints its credentials, and starts
`oauth2-proxy` against a `just run-dev` server on `127.0.0.1:3004`.

```sh
brew install oauth2-proxy      # if absent
just run-dev &                 # the provider, on :3004
tools/oidc-rp.sh               # registers the client and starts the proxy on :4180
```

Then open `http://127.0.0.1:4180/`, which redirects into `/oidc/authorize`,
through the consent page, and back to the proxy's callback. The proxy's
`--provider=oidc --provider-display-name` and `--oidc-issuer-url` are set by
the script; `--redirect-url` is `http://127.0.0.1:4180/oauth2/callback`, which
is what the script registers.

Two things a reader should know about the shape:

* `oauth2-proxy` sends PKCE only with `--code-challenge-method=S256`, which the
  script sets. Without it the authorization endpoint refuses with
  `invalid_request`, correctly — PKCE is required of every client type here.
* The proxy's default `--email-domain` is nothing, so `--email-domain=*` is set
  in the script; otherwise it authenticates the person and then refuses them
  for the address, which reads like a provider failure and is not one.
* `--insecure-oidc-allow-unverified-email` is set for the same shape of reason
  and is the honest one: this deployment reports `email_verified` truthfully,
  and a dev address is never verified because nothing mails it. A deployment
  does not set this; `VerifyEmail` moves the claim to `true`.
* **No `backchannel_logout_uri` is registered for it.** oauth2-proxy 7.x does
  not implement Back-Channel Logout — its `/oauth2/sign_out` is the
  front-channel route and answers `302`, which is not the `200` the
  specification asks of a logout endpoint. The receipt is proved against a real
  receiver in `crates/server/tests/oidc/rp.rs` instead.

Screenshots of the click-through are in `docs/review/o1/`.
