# Multiprocess runtime

`ronitnath` ships one container with three operating-system processes: the
supervisor, the Next HTTP child, and the DBOS worker child. The supervisor owns
signals and the total 45-second shutdown deadline. Either child exiting or
missing heartbeats fails the whole container. Docker owns restart policy.

## Local operation

Set `DATABASE_URL`, `ID_KEY`, `AUTH_SECRET`, and `PUBLIC_ORIGIN`. Production also
requires a build-specific `APP_VERSION` and stable per-machine `REPLICA_ID`.
`DBOS_SYSTEM_DATABASE_URL` defaults to `DATABASE_URL`; DBOS uses the `dbos`
schema and its own connection pool.

Apply both schemas explicitly before startup:

```sh
pnpm db:migrate:all
pnpm build
pnpm start
```

`pnpm dev` runs the same supervisor through tsx watch. A source change restarts
the local process tree; workflow modules are never registered inside Next.

`/healthz` is process/database liveness. `/readyz` additionally requires the
domain-event triggers, current DBOS schema, a ready worker, and a runtime that
is not draining. Deploy routing should use readiness; the existing edge health
probe may continue to use liveness.

## Mail recovery

Authentication callbacks persist `mail_intent` before returning. The worker
polls pending rows and starts a stable DBOS workflow. Six delivery attempts use
1, 5, 30, 120, and 300-second waits. Provider 429/5xx and ambiguous network
outcomes retry; ordinary 4xx responses stop. Because useSend has no accepted
idempotency contract, a crash after provider acceptance can duplicate a letter.

Inspect non-success rows with `pnpm mail:intents list`. An intent is manually
retryable only while its body still exists and its link has not expired:
`pnpm mail:intents retry <uuid>`. Accepted, exhausted, permanently rejected,
and expired intents have their token-bearing bodies removed.

DBOS workflow history is the recovery record. Do not reset domain-event cursors
to retry a workflow. Keep old application versions runnable until their pending
work is drained or explicitly migrated.

## Shared-library boundaries

- `fleet-events`: transactional facts, authorized replay frames, presence
  descriptors, and explicitly disposable PostgreSQL listeners.
- `fleet-configured`: emits a transactional `configured/published` fact.
- identity and gates: request identity/authorization only; no workflow startup.
- billing, ledger, accounting: authoritative transactional quantities and
  idempotent financial records; notification side effects consume their facts.
- `fleet-workflows`: DBOS itself plus replay-safe event dispatch and an explicit,
  cancellable subscription loop. Applications own domain interpretation,
  recipient authorization, message text, and retained evidence.
- `fleet-runtime`: lifecycle state and IPC types. Imports never install signals.

Rebuild the three checked-in local package artifacts with
`scripts/pack-grid.sh`; it also rewrites `vendor/SHA256SUMS`. No dependency may
point at a sibling source worktree.
