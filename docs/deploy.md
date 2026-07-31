# Manual OCI publication and deployment

ronitnath.com is packaged as one immutable OCI image containing the `site` and
`admin` binaries plus the Vite output. Forgejo's registry is the artifact
source; Nexus runs the exact registry digest through
[`deploy/compose.yaml`](../deploy/compose.yaml). Publication produces an
artifact, but never deploys it. Production changes are deliberately performed
by an operator following this document; there is no deployment or reconciler
script.

Production runs the OCI runtime. The root-owned runtime definition, exact
image reference, and runtime-owned secret copies live beneath
`/data/apps/ronitnath/oci/`; an ordinary repository checkout is never a runtime
input. Webdeploy manifests, shims, migration mirrors, generated units, release
pointers, and webdeploy backup hooks are retired together and are not a
rollback path.

The public listener remains `10.0.0.1:3130`. The admin listener remains
mesh-only at `100.88.31.199:3131`. Do not widen either bind.

## Build and publish on Alien

Alien is the build host. Its rootless Podman graph root is
`/data/podman-storage`, on the secondary disk. Cargo and pnpm cache mounts in
the Dockerfile therefore persist on that disk as part of Podman's build
storage. Forgejo's host-native verification runner remains socketless and
secretless; it does not build, publish, or deploy images.

Use a clean checkout at the exact revision that passed CI:

```sh
ssh alien
cd ~/dev/personal/ronitnath
git fetch origin
git checkout --detach <full-commit>
test -z "$(git status --porcelain)"

revision=$(git rev-parse HEAD)
image="git.isoastra.com/ronitnath/ronitnath:git-$revision"
time podman build \
  --jobs=4 \
  --format oci \
  --build-arg "SOURCE_GIT_HASH=$revision" \
  --label "org.opencontainers.image.created=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --tag "$image" \
  .
```

Authenticate interactively with a Forgejo package-write token. Do not put the
token in shell history, an environment file, or the repository:

```sh
podman login git.isoastra.com
digest_file=$(mktemp)
podman push --digestfile "$digest_file" "$image"
digest=$(cat "$digest_file")
rm -f "$digest_file"
printf '%s@%s\n' "${image%:*}" "$digest"
```

Record the resulting `git.isoastra.com/ronitnath/ronitnath@sha256:...` value.
That digest, the source revision, the successful Forgejo CI run, and the build
log form the release evidence. A tag is only a locator and must never appear in
the production image file.

The first build downloads base images and compiles every dependency. Repeat
the exact build to measure the warm baseline. For tuning, separately measure a
UI-only source change, a Rust leaf-source change, and a lockfile change. The
expected invalidation boundaries are:

| Change | Reused work | Rebuilt work |
|---|---|---|
| No source change | all image layers | manifest/tag only |
| `ts/` only | Cargo Chef and Rust binary | UI build and final image |
| Rust source only | Cargo Chef dependencies and pnpm | affected application crate and final image |
| `pnpm-lock.yaml` | Cargo Chef and Rust binary | pnpm fetch/install, UI, final image |
| `Cargo.lock`/`Cargo.toml` | pnpm | Cargo Chef dependency layer, binary, final image |

Do not add sccache until these measurements show application-crate codegen is
the remaining bottleneck. Cargo Chef plus the local registry/target layers is
the simpler first optimization.

Pilot measurements on Alien (2026-07-24, Podman graphroot and all cache mounts
on `/data`) validate that choice: the initial unprimed build was 214.5s; an
all-layer warm build with four stage jobs was 21.3s; after adding the persistent
Cargo target mount, a Rust leaf edit spent 2.0s compiling and 38.3s end to end;
a UI-only edit rebuilt pnpm/Vite without invoking Cargo and took 39.4s without
parallel stage scheduling. The runtime image is 132.8 MB. Most remaining warm
time is Podman/layer bookkeeping on the HDD, not Rust compilation, so sccache
would add operational state without attacking the measured bottleneck.

## Nexus registry credential

Nexus pulls with the dedicated Forgejo token `nexus-registry-pull-20260724`,
whose only scope is `read:package`. Its encrypted source is
`forgejo-registry-pull.age` in the keys repository. It cannot publish packages
or mutate repositories and is never provided to Forgejo Actions.

Materialize it and create Docker's derived root-only authentication file:

```sh
# Materialize the approved host registry-pull credential through its managed key source.
sudo install -d -o root -g root -m 0700 /etc/ronitnath/docker-auth
sudo docker --config /etc/ronitnath/docker-auth login \
  --username ronitnath \
  --password-stdin \
  git.isoastra.com < /etc/ronitnath/forgejo-registry-pull.token
sudo stat -c '%U:%G %a %n' \
  /etc/ronitnath/forgejo-registry-pull.token \
  /etc/ronitnath/docker-auth/config.json
```

Both files must be `root:root` and inaccessible to other users. Rotation is:
mint a second `read:package` token, update the encrypted blob, materialize,
repeat `docker login`, prove an exact-digest pull, then revoke the old token.

## Persistent-volume preflight

Production data is durable SQLite and uploaded photos. The only writable
application mount is the absolute host directory
`/data/apps/ronitnath/state`, presented as `/state`. The OIDC provider file is
copied as a runtime-owned `0400` secret to
`/data/apps/ronitnath/oci/secrets/oidc_providers.json` and is an absolute,
read-only bind. Compose sets `create_host_path: false`, so a typo
or missing secondary disk fails instead of silently creating an empty root-disk
directory.

Before every first deployment on a host, and after storage work, run:

```sh
findmnt --mountpoint /data
test "$(findmnt -no FSTYPE /data)" = ext4
sudo test -d /data/apps/ronitnath/state
sudo test -f /data/apps/ronitnath/state/app.db
sudo test -d /data/apps/ronitnath/state/photos
sudo test -f /data/apps/ronitnath/oci/secrets/oidc_providers.json
sudo stat -c '%u:%g %a %n' \
  /data/apps/ronitnath/state \
  /data/apps/ronitnath/state/app.db \
  /data/apps/ronitnath/state/photos \
  /data/apps/ronitnath/oci/secrets/oidc_providers.json
```

The state tree must be owned by the dedicated runtime identity `986:985` and
must not be group/world-writable. Correct ownership deliberately if the
preflight identifies drift; do not recursively chmod as a workaround.

Resolve and inspect the exact Compose model before starting anything:

```sh
sudo install -d -o root -g root -m 0755 /data/apps/ronitnath/oci
sudo install -d -o ronitnath-app -g ronitnath-app -m 0700 \
  /data/apps/ronitnath/oci/secrets
sudo install -o root -g root -m 0600 /dev/null \
  /data/apps/ronitnath/oci/image.env
sudoedit /data/apps/ronitnath/oci/image.env
# Exactly one line, using the recorded digest:
# RONITNATH_IMAGE=git.isoastra.com/ronitnath/ronitnath@sha256:...

compose='sudo docker --config /etc/ronitnath/docker-auth compose --env-file /data/apps/ronitnath/oci/image.env -f /data/apps/ronitnath/oci/compose.yaml'
$compose config --quiet
$compose config | grep -F 'source: /data/apps/ronitnath/state'
$compose pull
$compose run --rm --no-deps --entrypoint /bin/sh site -c \
  'test -w /state && test -r /state/app.db && test -d /state/photos'
```

This is the standard persistent-bind contract: mounted parent verified,
absolute source, pre-created exact path, `create_host_path: false`, numeric
runtime ownership, read-only root filesystem, least-writable mount, and an
in-container access check as the real runtime identity. A relative bind, named
volume for authoritative data, or Docker-created host directory is not an
acceptable production substitute.

## Deploy

SQLite migrations run when `site` starts. Before changing traffic, take and
integrity-check a consistent pre-deploy snapshot using the existing backup
procedure. Migrations shipped in release N must remain tolerable to the N-1
binary; database restore is never part of ordinary rollback.

From the root-owned OCI runtime definition on Nexus:

```sh
compose='sudo docker --config /etc/ronitnath/docker-auth compose --env-file /data/apps/ronitnath/oci/image.env -f /data/apps/ronitnath/oci/compose.yaml'
$compose config --quiet
$compose pull
$compose up -d --wait
$compose ps
```

Verify the exact digest and bind mounts, then both direct and public behavior:

```sh
$compose images
docker inspect ronitnath-site-1 --format '{{range .Mounts}}{{println .Source "->" .Destination .RW}}{{end}}'
docker inspect ronitnath-admin-1 --format '{{range .Mounts}}{{println .Source "->" .Destination .RW}}{{end}}'
curl -fsS http://10.0.0.1:3130/healthz
curl -fsS http://100.88.31.199:3131/healthz
curl -fsS https://ronitnath.com/healthz
```

Confirm the health response revision equals the image's OCI revision label and
the intended commit. Inspect recent logs for migration, SQLite, OIDC, and trace
export failures:

```sh
$compose logs --since 10m site admin
docker image inspect "$(grep '^RONITNATH_IMAGE=' /data/apps/ronitnath/oci/image.env | cut -d= -f2-)" \
  --format '{{index .Config.Labels "org.opencontainers.image.revision"}}'
```

Keep the previously verified digest recorded until the rollback window closes.

## Roll back

Replace `RONITNATH_IMAGE` in the root-owned image file with the previous exact
digest, then repeat the same pull, start, mount, digest, direct-health, and
public-health checks:

```sh
sudoedit /data/apps/ronitnath/oci/image.env
compose='sudo docker --config /etc/ronitnath/docker-auth compose --env-file /data/apps/ronitnath/oci/image.env -f /data/apps/ronitnath/oci/compose.yaml'
$compose pull
$compose up -d --wait
```

Rollback changes only the image. Never restore `app.db` during routine
rollback. A database restore is a separate disaster-recovery operation with
its own explicit evidence and traffic gate.

## Executed OCI cutover — 2026-07-31

- Artifact: `git.isoastra.com/ronitnath/ronitnath@sha256:92d1f2b331794364b33042bd4ee970b2b10159fba11e306f9ffe4ac3d4a76fbd`, revision `8f62de73323689cab100b268bca0e5ea730bd834`.
- Runtime: Nexus Compose project `ronitnath`; state remains
  `/data/apps/ronitnath/state`; the OIDC configuration is the runtime-owned
  read-only bind under `oci/secrets/`.
- Backup gate: a consistent SQLite `.backup`, compressed tar, SHA-256 manifest,
  tar listing, extraction, and `PRAGMA quick_check` restore drill passed at
  `/data/archives/ronitnath-pre-oci-20260731/`. A disposable OCI boot against
  that restored state passed for both listeners.
- Routing: the service-ingress catalog routes the public site to
  `10.0.0.1:3130`; its deployed shadow generation was valid on Nexus and Tor.
  No Caddy file was edited.
- Retirement: `ronitnath-{site,admin}.{service,socket}`, webdeploy releases,
  release pointers, migration snapshots, old runtime credential material, and
  repository webdeploy compatibility files were removed immediately after
  verification. `ronitnath-identity` is a separate Rauthy deployment and was
  intentionally retained.
- Rollback: the prior OCI candidate is
  `git.isoastra.com/ronitnath/ronitnath@sha256:9e243cd056059407e6d59bc2f74ca6d0a067f7209b07bfb8e1ba553bc1b5d3e9`
  (revision `0ce6e9d973698ae59453dd2d28fd38b1289d289c`). Set it in
  `/data/apps/ronitnath/oci/image.env`, then run the ordinary digest pull and
  Compose `up -d --wait` sequence above. Native webdeploy is not a rollback
  path.
