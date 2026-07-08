# OIDC factor

`stage_2` can act as an OpenID Connect relying party. With no provider file, no SSO UI is rendered and the auth routes stay inert.

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

## Flow

- Start: `GET /auth/oidc/{key}/start`
- Callback: `GET /auth/oidc/{key}/callback`
- PKCE S256 is always used.
- `state`, `nonce`, PKCE verifier, intent, and redirect URI are stored server-side in `pending_auth` for 10 minutes.
- ID tokens are validated with the `openidconnect` crate (issuer, audience, expiry, nonce, signature) and `at_hash` is checked when present.

## Settings

Configured providers render link buttons on `/settings`. Linking uses the same start/callback routes with link intent bound to the current session. If the subject is already linked to any identity, the link is rejected. Unlinking uses the existing factor removal route and last-factor guard.
