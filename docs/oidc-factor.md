# OIDC factor

`web_template` can act as an OpenID Connect relying party. With no provider file, no SSO UI is rendered and the auth routes stay inert.

## Config

Set `OIDC_PROVIDERS_PATH` (default: `data/oidc_providers.json`) to a JSON array:

```json
[
  {
    "key": "isoastra",
    "display_name": "Isoastra SSO",
    "issuer_url": "https://idp.example.test",
    "client_id": "stage2-local",
    "client_secret": "replace-with-client-secret",
    "scopes": ["openid", "profile", "email"],
    "auto_provision": true
  }
]
```

Production `ronitnath.com` uses the personal-universe Rauthy client, stored
outside the repository at `/data/apps/ronitnath/oidc_providers.json`:

```json
[{"key":"ronitnath-id","display_name":"Ronit Nath ID","issuer_url":"https://id.ronitnath.com/auth/v1/","client_id":"ronitnath-events","client_secret":"materialized-secret","scopes":["openid","profile","email"],"auto_provision":false}]
```

The public router's guest-claim flow is the only provisioning path. Keep
`auto_provision` false and `AUTH_SIGNUP=closed`; a bare guest login still needs
an existing live `person_identity_links` binding.

Fields:

- `key`: URL-safe route key used in `/auth/oidc/{key}/start`.
- `display_name`: rendered as `Continue with {display_name}`.
- `issuer_url`: exact OP issuer; discovery and ID-token `iss` must match.
- `client_id` / `client_secret`: confidential client credentials.
- `scopes`: optional; defaults to `openid profile email`; `openid` is forced if omitted.
- `auto_provision`: optional, default `true`.

Discovery runs at startup and caches provider metadata/JWKS in process.

## Trust model

A configured provider is a signup trust grant. If `auto_provision=true`, the first valid ID token for a new `{issuer}#{sub}` creates a human identity, personal account, owner membership, and `oidc` factor in one transaction. This is independent of `AUTH_SIGNUP`; closing password signup does not close configured SSO signup.

No email auto-linking is performed. Email claims may seed display/account metadata, but the lookup key is always the stable OIDC subject: `external_id = "{issuer}#{sub}"`.

### Public guest sign-in policy

The public site uses a separate `guest_login` OIDC intent and never treats
`auto_provision` as permission for open guest signup. A known OIDC factor may
create a public session only when its identity has one active
`person_identity_links` binding to this site's owner account; the session is
issued against that identity's existing `purpose = 'guest'` account.

For a first-time `{issuer}#{sub}`, provisioning is allowed only when OIDC was
started from a live, person-specific `/e/{token}/claim` capability and that
person remains unclaimed at callback time. The callback revalidates the link
before atomically creating the normal guest identity, guest account, owner
membership, OIDC factor, person link, and session. A bare public login, a
shared/revoked invite, an already-claimed person, or an OIDC identity without
an active person link fails closed. Email claims are never used for matching.

## Flow

- Start: `GET /auth/oidc/{key}/start`
- Callback: `GET /auth/oidc/{key}/callback`
- PKCE S256 is always used.
- `state`, `nonce`, PKCE verifier, intent, and redirect URI are stored server-side in `pending_auth` for 10 minutes.
- ID tokens are validated with the `openidconnect` crate (issuer, audience, expiry, nonce, signature) and `at_hash` is checked when present.

## Settings

Configured providers render link buttons on `/settings`. Linking uses the same start/callback routes with link intent bound to the current session. If the subject is already linked to any identity, the link is rejected. Unlinking uses the existing factor removal route and last-factor guard.
