# Production deployment (Nix + systemd)

Production is a **release** deployment with **durable** SQLite and photo data.
`flake.nix` builds one package containing both Rust binaries and the four
esbuild entry bundles. systemd runs the binaries as the dedicated
`ronitnath-app` account; the human `ronitnath` account remains the operator.

The checked-in `docker-compose.yml` is the previous deployment path. It is not
used for normal releases after cutover, but is retained as the immediate
container rollback path until that fallback is intentionally retired.

## Layout and network boundary

- Nix profile: `/nix/var/nix/profiles/ronitnath`
- SQLite: `/data/apps/ronitnath/app.db`
- Uploaded photos: `/data/apps/ronitnath/photos/`
- Optional OIDC provider registry:
  `/data/apps/ronitnath/oidc_providers.json`
- Packaged, read-only static tree:
  `/nix/var/nix/profiles/ronitnath/share/ronitnath/static/`
- Public site: `10.0.0.1:3130` on `wg0`, reached only by the nanode origin
- Admin: `100.88.31.199:3131` on the NetBird mesh

The single explicit bind in each unit deliberately replaces Docker's two port
publishes. The site does not need a host-loopback listener: nanode terminates
public TLS and reaches the private `wg0` address. The admin process has only a
mesh listener and is not exposed through public ingress. Do not change either
unit to `0.0.0.0`.

Both services share the database and photo directory. `site` creates the
SQLite file and applies migrations. `admin` only opens a migrated database, so
its unit wants and starts after `ronitnath-site.service`; its restart policy
also covers the short interval while a new site migration finishes.

## First build and initialization

Run from a clean checkout of the exact release revision on the x86_64 Linux
host. This repository currently has no `rust-toolchain.toml`, so the flake uses
nixpkgs' matched Rust platform directly. It uses the committed `.sqlx` cache
with `SQLX_OFFLINE=true` and invokes `ts/build.sh` with nixpkgs esbuild; Node,
`npm`, and other JavaScript package managers are not build or runtime inputs.
The crate has no private Git dependencies, so `buildRustPackage` needs only
`Cargo.lock` and does not consume the operator's GitHub credentials.

Generate and commit the input lock before the first deployment (the Windows
authoring machine does not have Nix):

```sh
nix flake lock
git add flake.lock
git commit -m 'build: lock Nix inputs'
nix build .#default
test -x ./result/bin/site
test -x ./result/bin/admin
ls ./result/share/ronitnath/static/dist/{site,guestbook,event_rsvp,events_admin}.js
```

The one-time initialization creates the service account and data directories,
installs both units, and enables them without starting them:

```sh
./deploy/deploy.sh init
```

`deploy/deploy.sh` resolves privileged helpers such as `useradd` and
`nix-env` before invoking `sudo`. This is required because sudo's clean PATH on
the target does not include the multi-user Nix profile.

## Cut over from Docker Compose

Build and initialize first so the downtime window contains only shutdown,
data copy, activation, and verification. Run these commands from the checkout
whose `./data/` is mounted by the old Compose deployment:

```sh
nix build .#default
./deploy/deploy.sh init

docker compose down
stamp=$(date -u +%Y%m%dT%H%M%SZ)
sudo install -d -m 0750 -o ronitnath-app -g ronitnath-app \
  /data/apps/ronitnath/backups
sudo cp -a ./data "/data/apps/ronitnath/backups/compose-$stamp"
sudo rsync -a ./data/ /data/apps/ronitnath/
sudo chown -R ronitnath-app:ronitnath-app /data/apps/ronitnath

./deploy/deploy.sh deploy
./deploy/deploy.sh status
curl --fail --show-error http://10.0.0.1:3130/healthz
curl --fail --show-error http://100.88.31.199:3131/healthz
```

Then verify the public `https://ronitnath.com` path through nanode and verify an
authenticated admin flow over the mesh. The existing ingress destination does
not change: only the process owning the two host sockets changes.

Do not run Compose and the systemd services together: they contend for the
same addresses and may write the same logical database. `docker compose down`
is therefore the explicit cutover gate.

## Routine deploys

From a clean, updated checkout with the committed `flake.lock`:

```sh
nix build .#default
./deploy/deploy.sh deploy
./deploy/deploy.sh status
curl --fail --show-error http://10.0.0.1:3130/healthz
curl --fail --show-error http://100.88.31.199:3131/healthz
```

`deploy` builds `.#default`, points the dedicated Nix profile at `./result`,
then restarts site first and admin second. The profile and packaged static tree
are read-only. All runtime writes remain under `/data/apps/ronitnath`.

Before a release containing migrations, take and verify a database backup.
Changing profile generations does not reverse a durable SQLite migration.

## Rollback

For a package-only rollback, switch to the preceding retained Nix profile
generation and restart both services:

```sh
./deploy/deploy.sh rollback
./deploy/deploy.sh status
```

Inspect all retained generations with:

```sh
sudo /nix/var/nix/profiles/default/bin/nix-env \
  --profile /nix/var/nix/profiles/ronitnath --list-generations
```

If the systemd cutover itself must be reverted, stop both units, copy the
current durable state back to Compose's bind-mounted directory, and bring the
old deployment up. This is the explicit **Compose rollback = `compose up`**
path:

```sh
sudo systemctl disable --now ronitnath-admin.service ronitnath-site.service
sudo rsync -a --delete --exclude backups/ /data/apps/ronitnath/ ./data/
sudo chown -R "$(id -u):$(id -g)" ./data
docker compose up -d
docker compose ps
```

If the new release applied an incompatible migration, restore the verified
pre-cutover backup instead of copying the migrated database back. Decide that
migration recovery path before cutover; neither Nix generation rollback nor
Compose rollback transforms the schema.

## Operations

```sh
./deploy/deploy.sh status
sudo journalctl -u ronitnath-site.service -u ronitnath-admin.service -f
```

The units enable CPU, memory, I/O, and task accounting. Both use
`CPUWeight=100`, `MemoryHigh=30%`, `MemoryMax=60%`, `TasksMax=1024`,
`NoNewPrivileges=yes`, and `PrivateTmp=yes`.
