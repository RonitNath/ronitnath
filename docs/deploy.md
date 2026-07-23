# Production deployment

ronitnath.com is a `webdeploy` release deployment with durable SQLite and
photo data. The fleet-managed `/usr/local/bin/webdeploy` builds inside the
pinned Nix development shell, writes read-only versioned releases beneath
`/data/apps/ronitnath/releases/`, atomically moves `current` and `previous`,
and restarts through socket-activated systemd services.

The complete deployment contract is [`deploy/app.toml`](../deploy/app.toml):
service user, public and admin listeners, exact non-secret runtime environment,
service ordering, build commands, migrations, and backup/restore-drill units.
The public listener is `10.0.0.1:3130`; the mesh-only admin listener is
`100.88.31.199:3131`. Do not widen either bind.

The release build runs `pnpm install --frozen-lockfile` then `pnpm build` after
the locked Cargo build. Node and pnpm are supplied by `flake.nix`; do not rely
on a globally installed JavaScript toolchain or commit `node_modules`.

Run from a clean, current checkout on nexus:

```sh
git pull --ff-only
./deploy/deploy.sh status
./deploy/deploy.sh deploy
./deploy/deploy.sh rollback
```

The authoritative origin is the private Forgejo repository
`git.isoastra.com/ronitnath/ronitnath`. Every push and pull request to `main`
is built and tested by the persistent nexus runner. A green push to `main`
then starts the fleet-owned `forgejo-webdeploy@ronitnath.service`, which checks
out that exact Forgejo revision and performs the production deployment. GitHub
is an archival push mirror and is not read anywhere in this deploy path.

One-time adoption or manifest changes use the same shim. `init` displays all
planned diffs before a single confirmation and does not restart services:

```sh
./deploy/deploy.sh render --out-dir /tmp/ronitnath-units
./deploy/deploy.sh init
```

Backup and restore-drill jobs are normally timer-driven and may be run through
`webdeploy backup` and `webdeploy drill`. Host backup configuration remains
root-owned and outside this repository.

For an emergency binary fallback, inspect the ordinary release directories,
create a plain symlink to the selected release, replace `current` with it, and
restart both services. Prefer a staged link plus atomic rename:

```sh
sudo ln -s /data/apps/ronitnath/releases/<release> /data/apps/ronitnath/current.next
sudo mv -T /data/apps/ronitnath/current.next /data/apps/ronitnath/current
sudo systemctl restart ronitnath-site.service ronitnath-admin.service
```

This is binary-only rollback. Never restore the database as part of it.
