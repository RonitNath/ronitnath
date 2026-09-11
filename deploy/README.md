# Deploying ronitnath.com

The `deploy` branch is the production control plane. `@isoastra/fleet-delivery`
validates and tests the exact SHA, publishes digest-bound runtime and migration
artifacts, then rolls NYC before SFO through the restricted `host-command.sh`.
Pushes to `main` store work and do not start Actions. The manual commands below
are retained for recovery.

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
RN_IMPERSONATION=on                         # `off` removes SignInAs entirely
```

Mail is not optional in production: with neither `USESEND_API_*` nor `SMTP_URL` set the app falls back to
the dev transport, which writes verification and reset mail to a directory
inside the container instead of sending it. `MAIL_FROM` defaults to the value
above and only needs setting to change the sender.

`RN_IMPERSONATION` is the one switch over the operator's sharpest command.
Set to `off`, SignInAs is not drawn on a party's page and the action declines
like anything else that is not allowed; every other operator surface is
unaffected. Anything but `off` leaves it on, which is the default. The
commands that destroy or impersonate also require a re-authentication inside
the last ten minutes — for the OIDC operator that is a fresh round trip
(`prompt=login`), so the ZITADEL app must not be configured to skip the
password prompt for it.

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

### The star catalogue, once per environment

`sky_star_detail` is 3,087,894 rows built offline (`tools/starcat/README.md`).
It is not in the image — 257 MB compressed — and it is not in a migration: it
is data, loaded once after the migration that creates the table, and again only
when the build is refreshed. Skipping it is survivable; the detail panel then
answers with what a star's catalogue key says and nothing more.

Copy the build to the host, then load it from the same one-off container the
migration runs in (the loader is Node and `pg`, so the image needs nothing
added to it; it checks the file against `data/sky_star_detail.sha256` before it
loads a byte):

```sh
scp data/sky_star_detail.csv.zst alien:/data/crypt/ronitnath/
docker run --rm --network host \
  --env-file /data/crypt/ronitnath/web.env \
  -v /data/crypt/ronitnath/sky_star_detail.csv.zst:/data/sky_star_detail.csv.zst:ro \
  ghcr.io/ronitnath/ronitnath:"$TAG"-migrate \
  node scripts/load-sky.mjs /data/sky_star_detail.csv.zst
```

About 80 seconds, one transaction, a full replacement: a failure leaves the old
rows in place.

When a release only *fills* the catalogue — a couple of hundred Hipparcos stars
Gaia has no usable row for — the whole file does not move. `build_delta.py`
writes `data/sky_star_detail.delta.csv` beside the release, and it is loaded
the same way with `--delta`, which upserts rather than truncating:

```sh
scp data/sky_star_detail.delta.csv alien:/tmp/
sudo mv /tmp/sky_star_detail.delta.csv /data/crypt/ronitnath/
docker run --rm --network host \
  --env-file /data/crypt/ronitnath/web.env \
  -v /data/crypt/ronitnath/sky_star_detail.delta.csv:/app/data/delta.csv:ro \
  ghcr.io/ronitnath/ronitnath:"$TAG"-migrate \
  node scripts/load-sky.mjs --delta /app/data/delta.csv
```

Under a second, idempotent, and on alien only — the table reaches delenda
through Patroni. Do it before the restart: the new bundle asks about stars the
old table has no rows for. The table is about 2.5 GB with its indexes, so it is loaded on
one node and reaches the others through Patroni replication — the second host
runs the image, not the load.

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

Take a `pg_dump -Fc` into `/data/crypt/ronitnath/backups/` before a migration
that is not purely additive. `sky_star_detail` is excluded from those dumps
(`--exclude-table sky_star_detail`): it is 2.5 GB of data that is rebuilt from
a file that is checksummed and kept, and a backup of it is a copy of something
that is not lost.
