#!/usr/bin/env bash
# Register an OpenID client and run `oauth2-proxy` against this deployment.
#
# The proof that a real relying party works. `crates/server/tests/oidc` is an
# in-repo one and proves the flow against the surface; this is somebody else's
# implementation, which is a different question — an RP written to the
# specifications rather than to this deployment's reading of them.
#
#   just run-dev &        # the provider, on :3004
#   tools/seed.sh         # a platform operator, if there is none
#   tools/oidc-rp.sh      # registers the client and starts the proxy on :4180
#
# Then open http://127.0.0.1:4180/ and sign in. The proxy redirects into
# /oidc/authorize, through the consent page, and back to its own callback.
#
# No `backchannel_logout_uri` is registered, and that is not an omission:
# oauth2-proxy 7.x does not implement Back-Channel Logout. Its
# `/oauth2/sign_out` is the front-channel route and answers a `302`, which is
# not the `200` the specification asks of a logout endpoint — so registering it
# would mean two POSTs and a log line about a relying party that was never
# listening. The receipt is proved against a real receiver in
# `crates/server/tests/oidc/rp.rs` instead.
#
# Nothing here is written to a file. The client secret exists in this shell and
# in the proxy's argv for the life of the process, which is what a dev loop is;
# a deployment registers its clients through `/platform` and never through a
# script.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

issuer=${RN_OIDC_ISSUER:-http://127.0.0.1:3004}
proxy_port=${RN_OIDC_PROXY_PORT:-4180}
email=${RN_SEED_EMAIL:-operator@example.invalid}
password=${RN_SEED_PASSWORD:-an obviously fake dev password}
redirect="http://127.0.0.1:$proxy_port/oauth2/callback"

# Homebrew's formula is `oauth2_proxy` and the binary it installs is
# `oauth2-proxy`; the symlink is usually on PATH and the Cellar path is the
# fallback for a shell that has not picked it up yet.
proxy=$(command -v oauth2-proxy || true)
[ -n "$proxy" ] || proxy=$(ls /opt/homebrew/opt/oauth2_proxy/bin/oauth2-proxy 2>/dev/null || true)
[ -n "$proxy" ] || {
  echo "oidc-rp: oauth2-proxy is not installed — brew install oauth2_proxy" >&2
  exit 1
}
curl -fsS "$issuer/readyz" >/dev/null 2>&1 || {
  echo "oidc-rp: nothing is answering at $issuer — start it with 'just run-dev'" >&2
  exit 1
}

jar=$(mktemp -t rn-oidc-rp)
trap 'rm -f "$jar"' EXIT

# Sign in as the operator. Registering a client is operator work, and this
# script has no authority of its own: it holds a session like any browser.
status=$(curl -sS -o /dev/null -w '%{http_code}' -c "$jar" \
  -X POST "$issuer/auth/sign-in" \
  -H 'sec-fetch-site: same-origin' \
  -H 'content-type: application/x-www-form-urlencoded' \
  --data-urlencode "email=$email" \
  --data-urlencode "password=$password")
[ "$status" = "303" ] || {
  echo "oidc-rp: /auth/sign-in answered $status for $email — run tools/seed.sh first" >&2
  exit 1
}

# The signing keys. A deployment with none signs nothing, and a fresh dev
# database has none: rotating is what mints the first.
keys=$(curl -fsS "$issuer/oidc/jwks" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["keys"]))')
if [ "$keys" = "0" ]; then
  echo "oidc-rp: no signing key yet; minting one"
  curl -fsS -b "$jar" -X POST "$issuer/api/cmd/rotate-signing-key" \
    -H 'sec-fetch-site: same-origin' -H 'content-type: application/json' \
    --data "{\"key\":\"$(uuidgen | tr 'A-Z' 'a-z')\"}" >/dev/null
fi

# Register the client. The reply carries the secret once, on the call that
# minted it — the row keeps only its SHA-256, so a second call cannot get it.
registration=$(curl -fsS -b "$jar" -X POST "$issuer/api/cmd/register-client" \
  -H 'sec-fetch-site: same-origin' -H 'content-type: application/json' \
  --data @- <<JSON
{
  "key": "$(uuidgen | tr 'A-Z' 'a-z')",
  "client_name": "oauth2-proxy",
  "client_uri": "http://127.0.0.1:$proxy_port/",
  "redirect_uris": ["$redirect"],
  "post_logout_redirect_uris": ["http://127.0.0.1:$proxy_port/"],
  "token_endpoint_auth_method": "client_secret_basic",
  "grant_types": ["authorization_code", "refresh_token"],
  "scopes": ["openid", "profile", "email", "offline_access"],
  "trusted": false,
  "members_only": false
}
JSON
)

read -r client_id client_secret <<<"$(printf '%s' "$registration" | python3 -c '
import json, sys
result = json.load(sys.stdin)["result"]
print(result["client"], result.get("client_secret", ""))
')"
[ -n "$client_secret" ] || {
  echo "oidc-rp: the registration returned no secret: $registration" >&2
  exit 1
}
echo "oidc-rp: registered $client_id"

# `--code-challenge-method=S256` is not optional here: PKCE is required of
# every client type (RFC 9700 §2.1.1), and without it the authorization
# endpoint refuses with invalid_request — correctly.
#
# `--email-domain=*` is not optional either, and for a different reason: the
# proxy's default is to accept no domain at all, so it would authenticate
# somebody and then refuse them for their address, which reads like a provider
# failure and is not one.
#
# `--insecure-oidc-allow-unverified-email` is the third, and it is the honest
# one: this deployment reports `email_verified` truthfully, and a dev address
# is never verified because nothing mails it. The proxy refuses an unverified
# address by default — correctly — so a dev loop has to say it does not mind.
# A real deployment does not set this, and does not need to: `VerifyEmail`
# moves the claim to `true`.
exec "$proxy" \
  --provider=oidc \
  --provider-display-name="rn-site" \
  --oidc-issuer-url="$issuer" \
  --client-id="$client_id" \
  --client-secret="$client_secret" \
  --redirect-url="$redirect" \
  --http-address="127.0.0.1:$proxy_port" \
  --cookie-secret="$(head -c 32 /dev/urandom | base64 | tr '+/' '-_' | cut -c1-32)" \
  --cookie-secure=false \
  --email-domain='*' \
  --scope="openid profile email" \
  --code-challenge-method=S256 \
  --insecure-oidc-allow-unverified-email=true \
  --skip-provider-button=true \
  --upstream="static://200" \
  "$@"
