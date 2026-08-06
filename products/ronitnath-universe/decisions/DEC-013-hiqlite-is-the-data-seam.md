---
id: DEC-013
date: 2026-08-06
source: owner ruling at the data gate (EV-027), escalation 1 of `changes/pitch-1/data-model.md`
supersedes: PITCH-001 §1 "the PostgreSQL seam"; PKT-01's PostgreSQL constraint; DEC-006's LISTEN/NOTIFY candidate
---
Chose **hiqlite — embedded SQLite replicated by Raft across sfo, nyc and nexus — as the data seam for
product data**, over PostgreSQL/Patroni, resolving a contradiction that had gone unnoticed since the
pitch froze.

**The contradiction.** On 2026-08-05, PITCH-001 §1 committed the first stateful slice to "the
PostgreSQL seam", PKT-01 repeated it as a constraint, and DEC-006 named PostgreSQL `LISTEN`/`NOTIFY`
as the fan-out channel SSE would need. The same day, the owner directed hiqlite for this app — an
override of `monorepo.md`, scoped here — and that is what shipped. The override was issued when the
store held one disposable table, so it had never been tested against the pitch's words. The data gate
was the first place both documents were read together.

**Why hiqlite wins on this workload**, with what was measured rather than assumed (2026-08-06):

| | |
| --- | --- |
| **Reads are local** | SQLite on the serving node's disk, no network hop. The console is almost entirely reads. |
| **Writes cost 3ms** | Raft commit needs the leader plus one follower. Leader is nexus; nexus↔sfo is 3.0ms, nexus↔nyc is 70.2ms. So ≈3ms today, ≈70ms worst case with nyc as leader — and every write in this product is human-paced. |
| **`LISTEN`/`NOTIFY` has a substitute** | hiqlite ships a Raft-replicated event bus (`notify`/`listen`) behind its `cache` + `listen_notify_local` features. DEC-006's cost #1 — a shared channel for multi-instance SSE fan-out — is discharged without Postgres. Both features are currently **off** and must be enabled. |
| **No external dependency** | No Patroni, no credentials, no second failure domain. The replication seam is deployed and exercised on every release (failover drill: 90/90 during a 14s outage). |
| **The seam that mattered survives** | Plain SQL, no ORM — which is what `monorepo.md` actually protects. |

**Rules that follow, all of them things the build must now do:**

1. **Migrations are additive-only.** hiqlite pushes migrations through Raft at boot, so during a
   rolling deploy the new binary's schema lands while two nodes still run the old binary against it.
   Add tables, add nullable columns, add indexes; never drop, rename or narrow in one release. A
   destructive change is a two-release dance.
2. **Applied migration files are immutable.** An id or hash mismatch panics the node at boot — editing
   a shipped `.sql` takes down every node that restarts.
3. **Binary rollback is safe by construction**, given rule 1: hiqlite's migrator logs a warning and
   continues when a binary's migration set is shorter than what is applied.
4. **Backup must exist before the first product write.** The `backup` feature is off today, correctly,
   for a table rebuilt every boot. Product data changes that. hiqlite has a standard path (checked
   against 0.14.0 at the owner's prompt): a nightly cron writes a consistent local backup **on every
   node**, the leader encrypts and pushes to S3 if a bucket is configured, and restore is
   `HQL_BACKUP_RESTORE=file:…|s3:…` at boot on node 1. **Chosen shape**: enable `backup`, leave S3
   unconfigured in the app, and let the fleet's existing per-app restic→B2 job take the local backups
   off-node — that inherits `AppBackupStale`/`AppRestoreDrillStale` alerting and a weekly restore
   drill the predecessor site already uses, instead of standing up a second unmonitored path, and it
   keeps S3 credentials out of the app. Keep `HQL_BACKUP_KEEP_DAYS` short so restic owns retention.
   The one piece of real work is that the per-app timer set is webdeploy-shaped while universe is
   podman/playbook-deployed. PKT-09 delta.
5. **Scaling is an operator act.** Raft needs an odd membership and a majority; three nodes grow to
   five deliberately, never by an autoscaler.
6. **SQLite dialect is accepted.** `segment_answers` and `event_release.manifest` are JSON text
   columns rather than `jsonb`. Two places, both write-validated in code.

**Scope**: universe-ronitnath only. `monorepo.md`'s PostgreSQL/Patroni ruling stands everywhere else.

**What would change this**: a workload this schema does not have — analytical queries over the
response log, a second application needing the same data, or a write rate where Raft commit latency
stops being free. None is plausible for a personal event platform.
