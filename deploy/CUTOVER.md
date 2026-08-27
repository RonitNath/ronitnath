# Cutover: forming the rebuilt cluster

The rebuild does not migrate the live deployment's data. The schema is new, the
id scheme is new, and the data lifecycle is disposable-dev by ruling
(`docs/rebuild/plan.md`): the existing hiqlite state is **archived and then
discarded**, and the three voters form an empty cluster from nothing. Every
account, session and invitation link on the running site ends at that moment.

This file is the runbook. Nothing in it is automated and nothing in it belongs
in CD — an operator provisions, a pipeline cuts over.

Read `docs/README-ops.md` first for the node table and the ops surface.

## Before anything

- **This is an owner decision**, taken separately from the merge. Do not start
  it because a release is ready.
- **Freeze `deploy`.** No pushes and no queued runs while the cluster is being
  formed; a rollout landing mid-formation will find a fleet it cannot gate on.
- **Record the rollback target.** On each node, read the digest currently
  pinned:

  ```sh
  grep RN_SITE_IMAGE /data/apps/rn-site/oci/image.env
  ```

  Write all three down. They should agree. If they do not, the fleet was
  already half-rolled and that is the problem to fix first.
- **Have the new digest.** The release job publishes it; take it from the
  publish step's `--digestfile` output or the registry's
  `Docker-Content-Digest` header. `docker inspect` on alien reports a
  different digest after a push and is not a source.

## 1. Stop all three voters

Formation-by-replacement is not available here — the new build's readiness
shape is different and its state is incompatible — so every node stops before
any node starts. On each of nexus, nyc and delenda:

```sh
cd /data/apps/rn-site/oci
sudo docker compose -p rn-site --env-file image.env --env-file node.env down
sudo docker compose -p rn-site ps        # nothing running
```

## 2. Archive the state, then empty it

Per node. The archive is the only copy of the old deployment after this step,
so prove it before deleting anything.

```sh
stamp=$(date -u +%Y%m%d)
node=$(hostname -s)
sudo install -d -o root -g root -m 0700 "/data/backups/$stamp-rn-site-rebuild"
sudo tar -C /data/apps/rn-site -cf "/data/backups/$stamp-rn-site-rebuild/state-$node.tar" state
sudo tar -tf "/data/backups/$stamp-rn-site-rebuild/state-$node.tar" | head    # it lists
sudo du -sh "/data/backups/$stamp-rn-site-rebuild/state-$node.tar"            # it is not empty
```

Only once all three archives exist and list, empty the state directories:

```sh
sudo rm -rf /data/apps/rn-site/state
sudo install -d -o 9751 -g 9751 -m 0700 /data/apps/rn-site/state
```

A **fresh** directory, not a cleaned one. A retry on top of half-formed raft
WALs produces a membership that can never elect, and that failure reads as a
networking problem for about an hour.

## 3. Provision the new contract

From a current checkout on nexus, as the operator:

```sh
git -C <checkout> pull --ff-only
<checkout>/deploy/provision.sh
```

This pushes the new `compose.yaml` to all three nodes and mints the public-id
key once, into `/etc/rn-site/cluster-secrets/id-key` on nexus and
`/data/apps/rn-site/secrets/id-key` on every node. Confirm it landed:

```sh
sudo stat -c '%n=%U:%G=mode.%a' /data/apps/rn-site/secrets/id-key
```

Never print the value. The key is not rotatable: public ids are derived from
it on read and stored in no column, so a second `openssl rand` renames every
object the deployment will ever have. If a later provisioning run reports
generating it again, the first one did not persist and you are about to form a
cluster whose ids will change — stop and find out why.

## 4. Form the cluster

Three empty voters have no majority until the second one joins, so the
steady-state gates cannot be satisfied on the way in. The pipeline's preflight
(`deploy/verify-cluster.sh` with no argument) correctly refuses an unformed
fleet, which is why formation runs the playbook directly rather than through a
push to `deploy`:

```sh
sudo -u deployer -H sh -lc '
  cd /data/apps/deployer/fleet &&
  git pull --ff-only &&
  cd ansible &&
  ansible-playbook -i inventory.yml playbooks/rn-site-deploy.yml \
    -e rn_site_image_digest=sha256:<the-new-digest> \
    -e rn_site_source_git_hash=<the-sha> \
    -e rn_site_bootstrap=true
'
```

`rn_site_bootstrap=true` skips the quorum precondition, drops the post-cutover
gate to liveness plus the expected revision, and disables auto-rollback —
there is no previous digest that was ever running on this state, so a rollback
would restore nothing and hide the real failure. Never set it for an ordinary
release.

Run from `fleet/ansible/`: `ansible.cfg` is only honored from the directory
that contains it.

## 5. Verify

```sh
<checkout>/deploy/verify-cluster.sh <the-sha>
```

Every voter ready on both raft groups, every voter on the released revision,
and the public surface through the CDN: `/` and `/tokens.css` at 200 with the
revalidation contract, one real module per bundle, and 404 on `/api/realtime`,
`/manage` and `/pkg/rn-site.wasm`.

Then unfreeze `deploy`.

## 6. The first operator

The formed cluster has no platform operator, and nobody can grant one through
the product: `platform:* #operator @person` is the one grant with no actor.

`rn-site bootstrap-operator <email>` writes it, but it opens the database
directly and hiqlite holds an exclusive lock on its data directory — which is
why `tools/seed.sh` stops the dev server before running it. There is no moment
on a formed cluster at which that is possible: every voter is serving, and
stopping one to take a grant is stopping the deployment to take a grant.

So the running process takes it instead. Set the address on **one** node:

```sh
# On nexus only, in /data/apps/rn-site/deploy/node.env
RN_SITE__BOOTSTRAP_OPERATOR_EMAIL=<your address>
```

then restart that node's container. Nothing happens yet: the address names no
person until somebody registers it.

Register through the real `/auth` form on the public site, with that address.
The node that holds the variable sees the `Registered` event on the change feed
— its own, or another voter's — and writes the relation. It logs one line:

```
the configured address is now this deployment's platform operator
```

Confirm by loading `/platform`; it is a 404 to everybody else, so seeing the
Parties list *is* the confirmation.

The kernel refuses a second operator, so leaving the variable set is safe and
re-running the step does nothing. Remove it at your leisure; from here,
platform administration is `SetRole` and has an actor on every row.

## Rolling back

Only meaningful before anyone has used the new deployment; after that, a
rollback discards whatever was created on it.

1. Stop all three (step 1).
2. Restore each node's archive and put the directory back:

   ```sh
   sudo rm -rf /data/apps/rn-site/state
   sudo tar -C /data/apps/rn-site -xf "/data/backups/$stamp-rn-site-rebuild/state-$(hostname -s).tar"
   sudo chown -R 9751:9751 /data/apps/rn-site/state
   sudo chmod 0700 /data/apps/rn-site/state
   ```

3. Repin the digest recorded in *Before anything*, through the playbook with
   `-e rn_site_image_digest=<the-old-digest>` — not by editing `image.env`,
   which the deploy role owns.
4. `deploy/verify-cluster.sh <the-old-sha>`. The old build reports the old
   readiness shape, so this script will not pass against it; read the three
   `/readyz` bodies directly and confirm each names the restored revision.

Leave the archive in place afterwards. It is the only copy.
