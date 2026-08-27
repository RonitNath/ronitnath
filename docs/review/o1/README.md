# The click-through, leg O1 (2026-08-27)

`oauth2-proxy` 7.15.4 (`brew install oauth2_proxy`) against a `just run`
server on `127.0.0.1:3004`, driven by `tools/oidc-rp.sh`, clicked through by
hand and viewed. The point of it is that the relying party is somebody else's
implementation: `crates/server/tests/oidc` proves the flow against this
surface, and this proves it against an RP written to the specifications rather
than to our reading of them.

| | what it shows |
|---|---|
| `1-sign-in.png` | The proxy redirects into `/oidc/authorize`, which finds nobody signed in and round-trips to `/auth?next=…`. The registration form's new **Handle** field is visible. |
| `2-consent.png` | The consent page: the client's registered name, the scopes as plain words, Allow and Deny. Four scopes here, because the shot was taken with `offline_access` and `prompt=consent`. |
| `3-signed-in-at-the-rp.png` | Back at the proxy, authenticated. Code exchanged, id token verified by the proxy against the JWKS it fetched. |
| `4-userinfo-at-the-rp.png` | What the proxy learned: `user` is the pairwise `sub` (not any id of anything), `preferredUsername` is the handle, and the address. |
| `5-sign-out-confirmation.png` | `GET /oidc/end_session` with no `id_token_hint`: asked, not obeyed. |
| `6-consent-light.png` | The consent page in the light theme. |
| `7-consent-390-light.png` | The same at 390 wide. |
| `8-consent-390-dark.png` | And in the dark theme. |

What the run proved beyond the screenshots, from the two logs:

* discovery succeeded (`Performing OIDC Discovery...`, then
  `OAuthProxy configured for OpenID Connect Client ID: c_…`);
* PKCE S256 all the way through — the proxy is started with
  `--code-challenge-method=S256`, and without it the authorization endpoint
  refuses, correctly;
* RFC 9207 — every response carried `iss=http://127.0.0.1:3004`;
* the first run refused at the callback with
  `email in id_token (operator@example.invalid) isn't verified`, which is both
  sides being right: this deployment reports `email_verified` truthfully and a
  dev address is never proved. The script sets
  `--insecure-oidc-allow-unverified-email` and says why;
* signing out at `/oidc/end_session` ended the session — `/api/whoami`
  answered the uniform decline immediately afterwards.

**No `backchannel_logout_uri` is registered for oauth2-proxy**, and that is not
an omission: it does not implement Back-Channel Logout. Its `/oauth2/sign_out`
is the front-channel route and answers `302`, which is not the `200` a logout
endpoint owes — the first run registered it, the POSTs were delivered, and the
provider logged *a relying party was not told about a sign-out* twice, which is
the honest reading. The receipt is proved against a real receiver in
`crates/server/tests/oidc/rp.rs`.
