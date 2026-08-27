# Operating rn-site

Three replicas of one image behind `ronitnath.com`, each a voter in the same
hiqlite cluster. `procedures/ha-service.md` in the context repo owns the
doctrine; this file is what is true of this application.

| node | mesh IP | notes |
|---|---|---|
| nexus | 100.88.31.199 | holds the canonical secrets; provisioning runs here |
| nyc | 100.88.223.144 | |
| delenda | 100.88.252.202 | NixOS; gets its registry client pushed from nexus |

Port 3160 is the app, 8100 the raft protocol, 8200 the hiqlite API. All three
are published on the node's mesh IP, never `0.0.0.0` — so every health check
must curl the mesh address, not loopback, or it fails while Compose
simultaneously calls the container healthy.

## The ops surface

| route | who asks | what it means |
|---|---|---|
| `GET /healthz` | the edge, and Compose's healthcheck | the process is up. It never answers for the database: restarting a container because a leader election is in progress turns an election into a restart loop |
| `GET /readyz` | the rollout gates, `deploy/verify-cluster.sh` | `{status, node, version, raft: {sqlite: {...}, cache: {...}}}`. 200 only when both raft groups are healthy and at their full voter count; 503 with the same body otherwise |
| `GET /version` | anything | the source git hash this binary was built from |
| `X-RN-App-Version` | every response, on every route | the same stamp, so "which revision is nyc serving" is one `curl -I` against any URL |

`readyz` names both raft groups because hiqlite runs two of them with
independent membership: `sqlite` holds the data, `cache` holds ephemeral state.
The cache group's persisted directories (`logs_cache/`, `state_machine_cache/`)
are disposable — when only that group is wedged, stop every node, delete those
two directories everywhere, and restart. The sqlite group's state is not
disposable and is never deleted to fix a symptom.

There is no `/metrics`. It is a listed non-goal for this cut.

## Provisioning

An operator provisions; the pipeline only ever rewrites a digest. Run
`deploy/provision.sh [node ...]` from a checkout on nexus, as the operator, and
run it again after any change to `deploy/compose.yaml` or the node identities.
It is idempotent.

It creates `/data/apps/rn-site/{oci,state,secrets}` on each node, writes
`node.env` (node id, mesh IP, the peer map) and the compose file, installs the
registry credential helper on delenda, and mints three secrets on nexus if they
do not exist:

| secret | rotatable | what breaks if it changes |
|---|---|---|
| `hiqlite-raft` | yes, with care | the cluster partitions until every node has the new value |
| `hiqlite-api` | yes, with care | as above |
| `id-key` | **no** | every public id in the deployment. Ids are AES-128 ciphertext of `table_tag ‖ rowid`, derived on read and stored in no column, so a new key renames every object — every saved URL, every invitation link, every id in anyone's notes. Changing it is a migration with a redirect table, not a deploy |

Nothing in CD writes any of them, and `image.env` is owned by the deploy role
after the first run — editing it by hand rolls the host silently.

## The release path

Tier 1. `main` is where work lands; a push to `deploy` ships it. `deploy` is a
protected branch and advances only through a pull request.

1. **gates** on the zero-secret host-native runner: `trunk --version`,
   `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`,
   `cargo test --workspace --locked`, `cargo check --target
   wasm32-unknown-unknown` for every crate that reaches a bundle, `bash -n` on
   every script, `tools/size-gate.sh`. No secret, no credential, no deploy.
2. **publish** on the privileged deploy runner: `podman build` from
   `Containerfile`, stamped with the source git hash, pushed to the forge
   registry. The digest from `--digestfile` is the artifact; a tag is a
   locator and never reaches a runtime host.
3. **preflight**: `deploy/verify-cluster.sh` with no argument — every voter
   ready, both raft groups formed, before anything is disrupted.
4. **roll**: the fleet playbook, `serial: 1`, nexus then nyc then delenda,
   with a per-node health gate and automatic rollback to the previous digest.
5. **prove**: `deploy/verify-cluster.sh <sha>` — every node on the released
   revision, and the public surface through the CDN.

Step 5 is the one that catches a rollout that stopped half-way: per-node
success does not mean the fleet converged.

**Rollback** is repinning the previous digest, which the deploy role keeps as a
comment in `image.env`. The database is never restored automatically; a backup
exists for a deliberate operator decision, and the previous binary has to
tolerate the schema it finds.

**When the workflow is unavailable**, an authorized operator on alien runs the
same playbook by hand with an exact digest — `procedures/delivery.md`, §Operator
escape hatch. Never a tag, never a rerun of a release that already published.

## Things that presented as a different bug than they were

- **Cloudflare replaced the origin's `Cache-Control`** until the zone got a
  cache rule setting `browser_ttl.mode = respect_origin`. A freshness gate at
  the origin proves nothing about a visitor, which is why the public assertions
  in `verify-cluster.sh` go through the CDN.
- **The rollout scripts run on the CD runner's own PATH.** A NixOS systemd unit
  carries exactly the tools its `path = [...]` names. `verify-cluster.sh` needs
  curl, grep and tr; adding a fourth means rebuilding that runner first.
- **A failed formation leaves poisoned raft state.** Retrying on top of
  half-formed WALs produces a membership that can never elect. Archive the
  state directories and start empty — see `deploy/CUTOVER.md`.
- **The runtime image has no build tree.** Anything that reads a repo-relative
  file works under `cargo run`, passes every test, and dies only in the
  container. `RN_SITE__STATIC_DIR` exists for exactly this reason.
