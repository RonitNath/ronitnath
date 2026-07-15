# Production deployment (incremental releases + systemd)

Production is a **release** deployment with **durable** SQLite and photo data.
`flake.nix` is a toolchain-only dev shell (pinned Rust via rust-overlay,
`pkgs.esbuild`); it packages nothing. `deploy/deploy.sh deploy` compiles both
Rust binaries incrementally on the build host inside that shell, rebundles the
four esbuild entries, stamps a read-only release directory, and atomically
repoints the `current` symlink that both units resolve at start. systemd runs
the binaries as the dedicated `ronitnath-app` account; the human `ronitnath`
account remains the operator.

Each service has a paired socket unit (`ronitnath-{site,admin}.socket`) that
owns its listen socket. systemd keeps those sockets bound while the services
restart, so connections arriving during a deploy queue in the kernel instead
of being refused, and the binaries drain in-flight requests on SIGTERM — a
routine deploy refuses no connections. The processes adopt the activated
socket at start and fall back to binding `BIND_ADDR`/`ADMIN_BIND_ADDR` when
run without socket activation (local development).

The checked-in `docker-compose.yml` is the pre-2026-07-13 deployment path. It
is not used for normal releases, but is retained as the last-resort container
rollback path until that fallback is intentionally retired.

## Layout and network boundary

- Releases: `/data/apps/ronitnath/releases/<utc-stamp>-<rev>/`
  (root-owned, read-only to the service; `bin/site`, `bin/admin`, `static/`,
  and a `release.json` provenance manifest with binary/lockfile digests and
  the pre-migration snapshot path when one was created)
- Active release: `/data/apps/ronitnath/current` (atomic symlink; `previous`
  retains the prior release for rollback; the newest five plus both pointers
  survive pruning)
- SQLite: `/data/apps/ronitnath/app.db`
- Uploaded photos: `/data/apps/ronitnath/photos/`
- Optional OIDC provider registry: `/data/apps/ronitnath/oidc_providers.json`
- Public site: `10.0.0.1:3130` on `wg0`, reached only by the nanode origin
- Admin: `100.88.31.199:3131` on the NetBird mesh
- Each address appears twice by design: `ListenStream` in the socket unit
  (the live listener) and `BIND_ADDR`/`ADMIN_BIND_ADDR` in the service (the
  non-activated fallback). Change them together.

The single explicit bind in each unit deliberately replaces Docker's two port
publishes. The site does not need a host-loopback listener: nanode terminates
public TLS and reaches the private `wg0` address. The admin process has only a
mesh listener and is not exposed through public ingress. Do not change either
unit to `0.0.0.0`.

Both services share the database and photo directory. `site` creates the
SQLite file and applies migrations. `admin` only opens a migrated database, so
its unit wants and starts after `ronitnath-site.service`; its restart policy
also covers the short interval while a new site migration finishes.

## Build mechanics

The dev shell pins the toolchain; the checkout's `target/` directory carries
the compilation cache, and `[profile.release] incremental = true` keeps
release codegen incremental, so a shallow change recompiles in ~2s (whole
deploy ≈ 8s, of which 5s is the post-restart readiness gate). A fresh checkout
builds cold in ~2 minutes. The build uses the committed `.sqlx` cache with
`SQLX_OFFLINE=true` and invokes `ts/build.sh` with the shell's esbuild; Node,
`npm`, and other JavaScript package managers are not build or runtime inputs.

`deploy.sh` warns when the working tree is dirty and marks the release
`-dirty` in its directory name and manifest; deploy releases only from clean,
pushed revisions. `nix` is not on non-login/`sudo` PATHs on the target —
export `/nix/var/nix/profiles/default/bin` onto `PATH` first; the script
resolves privileged helpers such as `useradd` itself before invoking `sudo`.

## One-time initialization

Creates the service account, data and release directories, installs both
service units, both socket units, and the backup/restore-drill script and
units, then enables the services, sockets, and timers without starting them:

```sh
./deploy/deploy.sh init
```

Re-run `init` whenever the unit files change; it is idempotent and ends with
`daemon-reload`.

## Routine deploys

From a clean, updated checkout with the committed `flake.lock`:

```sh
git pull --ff-only
./deploy/deploy.sh deploy
./deploy/deploy.sh status
curl --fail --show-error http://10.0.0.1:3130/healthz
curl --fail --show-error http://100.88.31.199:3131/healthz
```

`deploy` builds incrementally, stamps the release, flips `current`, restarts
site first and admin second (the socket units keep both ports accepting
throughout), holds a 5-second readiness gate on both units, then prunes old
releases. All runtime writes remain under
`/data/apps/ronitnath`.

### Pre-migration snapshots

Pre-migration snapshots run only when a deploy carries pending migrations (or
an explicitly overridden schema divergence). Before the release pointer moves,
the deploy script uses SQLite's `VACUUM INTO` to capture one consistent snapshot
of the shared database while concurrent WAL writes continue, verifies it, and
aborts the deploy on any snapshot failure. The host must provide the `sqlite3`
CLI; the script retains the newest 10 pre-migration snapshots. Restoring one is
a deliberate manual decision, and nightly off-host backups remain a separate
recovery mechanism. Release rollback remains binary-only and does not reverse a
durable SQLite migration.

## Off-host backups

The host supplies exactly one backup configuration file that the application
never carries: `/etc/restic/env`, owned by root with mode `0600`. It defines
`RESTIC_REPOSITORY`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and
`RESTIC_PASSWORD`. `deploy/deploy.sh init` warns but succeeds when that file is
absent, so application installation does not depend on host backup provisioning;
the enabled backup timer then fails visibly until the host contract is present.

Each night at 03:30, with up to 30 minutes of randomized delay, the ronitnath
backup unit uses SQLite `VACUUM INTO` to create a consistent copy of the live
WAL database and requires `PRAGMA integrity_check` to return `ok`. Restic backs
up that copy and the application data root while excluding the live database,
its WAL/SHM companions, and the local pre-migration backup directory. The
retention pass keeps 7 daily, 4 weekly, and 6 monthly snapshots; it uses
`restic forget` without pruning.

Each Sunday at 04:30, also randomized, the restore-drill unit restores the
latest snapshot carrying the `ronitnath` tag into fresh scratch space. It
requires an intact restored database and at least one user table, then removes
the scratch data. A missing configuration, failed backup, or failed drill leaves
its oneshot unit failed so the host's failed-unit alert can surface it.

When a node-exporter textfile collector directory exists, the script writes
these gauges atomically: `ronitnath_backup_last_success_timestamp_seconds`,
`ronitnath_backup_exit_code`,
`ronitnath_restore_drill_last_success_timestamp_seconds`, and
`ronitnath_restore_drill_exit_code`. The last-success value remains the prior
successful epoch after a failure; exit code is zero only for a successful run.

Repository initialization, a weekly repository-wide `restic check`, pruning,
and alert rules for failed units, nonzero exit-code gauges, and stale
last-success gauges are host-level responsibilities. They live outside this
application repository.

Recovery targets: RPO: 24h nightly + deploy-time pre-migration snapshots; RTO: minutes via restic restore, drill-tested weekly.

## Rollback

Release rollback is binary-only; binaries tolerate a database schema newer than
their embedded migrations by design, so the N-1 binary boots against the N schema.

For a binary/asset rollback, swap `current` with `previous` and restart:

```sh
./deploy/deploy.sh rollback
./deploy/deploy.sh status
```

Inspect retained releases with `sudo ls -l /data/apps/ronitnath/releases/`.
The pre-2026-07-14 nix profile `/nix/var/nix/profiles/ronitnath` still holds
the last sandbox-built generation as a stale emergency fallback (its old unit
files would also need restoring); delete it once the release path has earned
full confidence.

If the systemd deployment itself must be reverted, stop both units, copy the
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
migration recovery path before cutover; neither release rollback nor Compose
rollback transforms the schema.

## Operations

```sh
./deploy/deploy.sh status
sudo journalctl -u ronitnath-site.service -u ronitnath-admin.service -f
```

The units enable CPU, memory, I/O, and task accounting. Both use
`CPUWeight=100`, `MemoryHigh=30%`, `MemoryMax=60%`, `TasksMax=1024`,
`NoNewPrivileges=yes`, and `PrivateTmp=yes`.
