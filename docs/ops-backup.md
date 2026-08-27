# Backup and restore

Requirement **F16.1** opens with a verification spike, and this file is it: what
hiqlite actually exposes for snapshot and restore, read before anything was
designed on top of it. Everything else in F16 depends on the answer, so the
answer comes first and the design comes after it.

**Version pinned:** `hiqlite = "0.14.0"`, as `Cargo.toml` names it. Read from
the crate source in the registry
(`~/.cargo/registry/src/*/hiqlite-0.14.0/`) rather than from memory, and
cross-checked against the crate's own documentation:

- <https://docs.rs/hiqlite/0.14.0/hiqlite/struct.Client.html>
- <https://github.com/sebadob/hiqlite#backups> (the README's *Backups* section)

## The finding, in one sentence

**hiqlite 0.14 has a backup and restore surface, and this build does not
compile it in — and even with it compiled in, it is a whole-node disaster
recovery mechanism, not a backup of the deployment's data.**

Both halves matter, and the second is the one that settles the design.

### Half one: the feature is off, and cannot be turned on by itself

`src/lib.rs`:

```rust
#[cfg(feature = "backup")]
mod backup;
```

and `Cargo.toml`:

```toml
backup = [
    "dep:cron",
    "s3",
    "sqlite",
]
s3 = ["backup"]
```

`backup` and `s3` imply each other. There is no way to have the local half
without the object-storage half, and enabling either pulls in `cron`, the S3
client, and hiqlite's `EncKeys` — which `init_enc_keys()` requires at startup
(`src/start.rs`), so a bare `cargo run` would then need a keyring. This
workspace's feature set is

```toml
hiqlite = { version = "0.14.0", default-features = false, features = [
    "sqlite", "auto-heal", "cache", "macros", "toml", "listen_notify_local",
] }
```

so in the binary we ship, `Client::backup`, `Client::backup_list_local`,
`Client::backup_file_local`, `Client::backup_list_s3`,
`Client::backup_s3_stream` and the whole `HQL_BACKUP_RESTORE` path **do not
exist**. The comment already in `Cargo.toml` — "no `backup` (it needs S3 and
EncKeys, which breaks a bare `cargo run`)" — is correct, and this spike
confirms it rather than overturning it.

### Half two: what it is, when it is on

Verbatim, from `src/client/backup.rs`:

> Create an on-demand backup of the SQLite state machine.
>
> You usually don't need to call this manually, because Hiqlite will
> automatically run a cron job every night and push backups to object storage,
> when you have the `s3` feature enabled.
>
> **Note:**
> Each raft node will create a backup on local disk, but only the current
> leader will encrypt and push it to s3 storage. This is why you will typically
> only see the leaders node id inside your bucket and not the other ones.
>
> The backup will be created in the background and run on other threads. This
> means it will not be finished immediately when this function returns.

What it writes is one file per node,
`backup_node_{node_id}_{ts}.sqlite`
(`src/store/state_machine/sqlite/writer.rs`) — a copy of the state-machine
SQLite file, and nothing else.

Restore is not an API at all. It is an environment variable read at boot, and
the README's three steps are:

> 1. Shut down the cluster.
> 2. Provide a backup file name on S3 storage with the `HQL_BACKUP_RESTORE`
>    value with prefix `s3:` (encrypted), or a file on disk (plain sqlite file)
>    with the prefix `file:`.
> 3. Start up the cluster again. After the restart, make sure to remove the
>    `HQL_BACKUP_RESTORE` env value.

`src/backup.rs` is where that happens, and three things in it are the reason it
is not what F16 asks for:

```rust
/// Check if the env var `HQL_BACKUP_RESTORE` is set and restores the given backup if so.
/// Returns `Ok(true)` if backup has been applied.
/// This will only run if the current node ID is `1`.
pub(crate) async fn restore_backup_start(node_config: &NodeConfig) -> Result<bool, Error>
```

* **Node 1 only.** Every other voter takes
  `fs::remove_dir_all(node_config.data_dir)` and rejoins as a blank follower.
* **It replaces the data directory.** `restore_backup` removes the db, the
  snapshots, the lock file and the raft logs, then copies the backup file into
  place. There is no merge and no partial restore; there is not even a refusal
  if the directory has data in it, because wiping it *is* the procedure.
* **Its only validation is that the file is a hiqlite state machine.**
  `is_metadata_ok` reads one row — `SELECT data FROM _metadata WHERE key =
  'meta'` — and deserializes it. It knows nothing about our schema version, our
  feed offset, or the id key the public ids in it were derived under, and it can
  be skipped outright with `HQL_BACKUP_SKIP_VALIDATION=true`.

## Therefore: the logical walk is what ships

F16.1 named the fallback in advance and said it "is the design, not a
disappointment". It is the design here for four reasons, only the first of
which is the feature flag:

1. It is not compiled in, and compiling it in costs an S3 client and a key
   management step on every `cargo run`.
2. **A file copy cannot name the offset it is consistent at.** F16.2's rule is
   that a backup which cannot say what it contains is not one. hiqlite's copy
   is consistent — it is a state-machine file — but nothing in it or beside it
   says *which feed offset* that is, and the offset is what makes the restored
   deployment's health screen legible (F16.3).
3. **It carries no id-key fingerprint.** Public ids are derived from
   `RN_SITE__ID_KEY` on read and stored in no column, so restoring a state
   machine under a different key silently renames every object in the
   deployment. That is the one failure F16.3 requires a *refusal* for, and a
   mechanism that does not record the key cannot refuse.
4. It is the same walk F17 needs, restricted to one owner subtree, so building
   it now means the export boundary is guessed at once instead of twice.

The design that ships is therefore the board's frame **E1**:

```
rn-site admin backup <dir>
    ReadStore, one offset
    → walk every table in dependency order
    → manifest: offset, schema hash, id-key fingerprint, per-table counts
    → rows

rn-site admin restore <dir>
    → refuses a non-empty database
    → refuses an id-key fingerprint that is not the manifest's
    → the restored offset is the feed head
```

`crates/server/src/admin/backup.rs` is that walk and `admin/restore.rs` is its
inverse; the manifest's shape is `admin::manifest::Manifest`.

### What the manifest carries, and what it deliberately does not

| field | why |
|---|---|
| `offset` | the audit id the walk was consistent at — F16.2's refusal is "no offset, no backup" |
| `schema` | sha-256 over the applied migration hashes, so a restore onto a different schema is caught before it writes |
| `id_key` | a **fingerprint** — sha-256 of the key, hex, first 16 characters. Never the key |
| `counts` | one row count per table, which is the acceptance test |
| `node`, `version`, `at` | which node wrote it, on which build, when |

The id key itself is never written, never printed and never logged. A backup
directory is readable by whoever holds it; a backup directory that contained
the id key would be a backup directory that could rename every object in any
deployment it was carried to.

## Operating it

```sh
tools/ephemeral.sh backup                 # into target/ephemeral/backups/<stamp>/
tools/ephemeral.sh backup /tmp/somewhere
tools/ephemeral.sh reset
tools/ephemeral.sh restore /tmp/somewhere
```

Both wrap `rn-site admin`, and both stop the instance first: the subcommands
open the data directory directly, and hiqlite holds an exclusive lock on it, so
a running voter is a refusal that names the pid holding the lock
(`crates/server/src/admin/lock.rs`).

On a *running* node the same actions are reachable over
`RN_SITE__ADMIN_ADDR`'s loopback-only listener — but `backup` and `restore` are
not among them, and that is deliberate: a walk taken while commits are landing
is consistent at no offset at all.

## What this does not cover

* **Production.** `deploy/CUTOVER.md` remains the runbook for the fleet, and
  its per-node `tar` of the state directory remains the archive it describes.
  This leg builds the tools; it does not run them anywhere but a local
  ephemeral instance.
* **Cross-deployment import.** F17 is deferred, and a restore that refuses an
  id-key mismatch is exactly the guard that keeps somebody from discovering
  that by accident.
