# Deploying ronitnath.com

Manual, `docker compose`, one host. No CI deploys anything; a deploy is a
person running these commands.

- Service: `web`, host network, listening on `PORT=3140`. The edge proxies
  `ronitnath.com` to `127.0.0.1:3140`.
- Config: `/data/crypt/ronitnath/web.env` on the target (root-owned, `0600`).
- Database: the per-node HAProxy in front of Patroni `internal-ha`, so the DSN
  in `web.env` dials `127.0.0.1:5000`.

## Database bootstrap (once, on the Patroni leader)

```sql
CREATE ROLE ronitnath LOGIN PASSWORD '<generated>';
CREATE DATABASE ronitnath OWNER ronitnath;
\connect ronitnath
REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT ALL ON SCHEMA public TO ronitnath;
```

Then `/data/crypt/ronitnath/web.env`, from `.env.example`:

```
DATABASE_URL=postgres://ronitnath:<generated>@127.0.0.1:5000/ronitnath
ID_KEY=<openssl rand -hex 16>       # never rotate: it is every public id
PUBLIC_ORIGIN=https://ronitnath.com
SESSION_COOKIE=rn_session
SESSION_TTL_DAYS=30
OIDC_ISSUER=https://auth.isoastra.com
OIDC_CLIENT_ID=...
OIDC_CLIENT_SECRET=...
OIDC_REDIRECT_URI=https://ronitnath.com/auth/oidc/callback
OIDC_ALLOWLIST=ronit@isoastra.com
USESEND_API_URL=https://mail.isoastra.com   # preferred: the fleet mailer's REST API
USESEND_API_KEY=<usesend api key>
SMTP_URL=                                   # fallback only; the useSend SMTP proxy cannot STARTTLS
MAIL_FROM=Ronit Nath <no-reply@ronitnath.com>
APP_VERSION=<tag>
```

Mail is not optional in production: with neither `USESEND_API_*` nor `SMTP_URL` set the app falls back to
the dev transport, which writes verification and reset mail to a directory
inside the container instead of sending it. `MAIL_FROM` defaults to the value
above and only needs setting to change the sender.

The ZITADEL app is `ronitnath` in the Isoastra org. Its redirect URI must be
registered there exactly as `OIDC_REDIRECT_URI` names it, and the callback
accepts no address but the one on `OIDC_ALLOWLIST`.

## Seed the operator (once, after the first migrate)

```sh
docker run --rm --network host \
  --env-file /data/crypt/ronitnath/web.env \
  ghcr.io/ronitnath/ronitnath:"$TAG"-migrate \
  pnpm seed:operator --email ronit@isoastra.com
```

It creates the operator person with no factors and grants it `operator` on
`platform:*`; the first ZITADEL sign-in attaches the real subject to it.
Running it twice changes nothing. Skipping it is survivable — the callback
provisions the same person and grant on first sign-in — but then the display
name is whatever ZITADEL sent.

## Release

`TAG` is the jj change id short form or the git commit sha — the same value
goes into `APP_VERSION`, so `/healthz` names exactly what is running.

Either build on the target:

```sh
export TAG=$(git rev-parse --short HEAD)
docker build --build-arg APP_VERSION="$TAG" -t ghcr.io/ronitnath/ronitnath:"$TAG" .
docker build --target migrate -t ghcr.io/ronitnath/ronitnath:"$TAG"-migrate .
```

or pull what was built elsewhere:

```sh
export TAG=<tag>
docker pull ghcr.io/ronitnath/ronitnath:"$TAG"
docker pull ghcr.io/ronitnath/ronitnath:"$TAG"-migrate
```

## Migrate, then start

Migrations never run at boot. Apply them from a one-off container first:

```sh
docker run --rm --network host \
  --env-file /data/crypt/ronitnath/web.env \
  ghcr.io/ronitnath/ronitnath:"$TAG"-migrate
```

Then start the service:

```sh
cd /data/crypt/ronitnath        # holds compose.yaml and web.env
TAG="$TAG" docker compose -f compose.yaml up -d
```

## Verify

```sh
curl -sS http://127.0.0.1:3140/healthz
# {"ok":true,"db":"ok","version":"<TAG>"}
docker inspect --format '{{.State.Health.Status}}' ronitnath-web   # healthy
curl -sSI https://ronitnath.com/ | head -1
```

## Rollback

Roll the image back to the previous tag; migrations are forward-only, so roll
back only to a tag whose schema the current database still satisfies.

```sh
TAG=<previous> docker compose -f compose.yaml up -d
curl -sS http://127.0.0.1:3140/healthz
```
