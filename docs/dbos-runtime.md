# Multiprocess runtime

`ronitnath` ships one container with three operating-system processes: the
supervisor, the Next HTTP child, and the DBOS worker child. The supervisor owns
signals and the total 45-second shutdown deadline. Either child exiting or
missing heartbeats fails the whole container. Docker owns restart policy.

## Local operation

Set `DATABASE_URL`, `ID_KEY`, `AUTH_SECRET`, and `PUBLIC_ORIGIN`. Production also
requires a build-specific `APP_VERSION` and stable per-machine `REPLICA_ID`.
`DBOS_SYSTEM_DATABASE_URL` defaults to `DATABASE_URL` and, when supplied, must
name that same database. DBOS uses the `dbos` schema and a separate bounded
pool. A PostgreSQL advisory lock rejects two live workers with the same
`REPLICA_ID`.

Apply both schemas explicitly before startup:

```sh
pnpm db:migrate:all
pnpm build
pnpm start
```

`pnpm dev` runs the same supervisor through tsx. Next owns route recompilation;
the supervisor watches worker and mail sources, quiesces only the old worker,
and forks a fresh worker after it exits. Workflow modules are never registered
inside Next or duplicated inside one worker process.

`/healthz` is process/database liveness. `/readyz` additionally requires the
domain-event triggers, the minimum compatible application migration, required
DBOS tables, a ready worker, and a runtime that is not draining. Runtime roles
have no DDL path. Deploy routing should use readiness; the existing edge health
probe may continue to use liveness.

Shutdown first quiesces both children and waits up to five seconds for their
acknowledgements. HTTP then drains for up to 15 seconds, followed by DBOS's
20-second workflow budget, all inside the supervisor's 45-second deadline.
Any failed child cleanup or forced termination makes the container exit nonzero.

## Mail recovery and migration

New authentication callbacks persist a `fleet_delivery_intent` and its
`fleet_notification_outbox` row in one transaction before returning. The
shared notification scheduler starts stable DBOS workflows. Authentication
mail is direct delivery: it creates no inbox notification and uses no
aggregation. Six logical delivery attempts use 1, 5, 30, 120, and 300-second
waits. The address and immutable rendered message are loaded inside worker
steps and are purged after acceptance, rejection, or expiry. Correlation,
message kind, a keyed recipient identity, and sanitized attempt evidence remain.

useSend receives the stable intent key as `Idempotency-Key`; set
`USESEND_IDEMPOTENCY_RETENTION_MS` only when the provider contract guarantees
deduplication for that period. SMTP receives a stable `Message-ID`, which does
not itself guarantee deduplication. The application explicitly retries
uncertain authentication delivery with the same key and payload. A crash after
provider acceptance but before the DBOS checkpoint can therefore duplicate a
message on a provider without deduplication.

Migration `0016_fleet_notifications` is additive. New callbacks write only the
shared tables. The registered legacy `deliverMail` workflow and legacy poller
remain solely for `mail_intent` rows created before the handoff. No row has two
dispatchers, and old application versions can recover their workflow history.

Use `pnpm mail:intents summary` for both stores, `list` for non-success rows,
and `show <uuid>` for sanitized attempt evidence. `resume <uuid>` is the legacy
same-version recovery command. `retry <uuid>` creates a new generation for a
paused shared intent or an eligible legacy intent while content and link remain
live. Concurrent operator actions serialize per intent.

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
- `fleet-notifications`: durable inbox state, aggregation primitives, revision
  proposals, delivery evidence, transport contracts, and the DBOS scheduler.
  ronitnath currently adopts only direct authentication delivery.
- `fleet-workflows`: DBOS itself plus replay-safe event dispatch and an explicit,
  cancellable subscription loop. Applications own domain interpretation,
  recipient authorization, message text, and retained evidence.
- `fleet-runtime`: lifecycle state and IPC types. Imports never install signals.

Rebuild the four checked-in local package artifacts with
`scripts/pack-grid.sh`; it also rewrites `vendor/SHA256SUMS`. No dependency may
point at a sibling source worktree.

`pnpm acceptance:local` runs the Grid gate and PostgreSQL integration, rebuilds
the exact tarballs, verifies their checksums, installs the frozen application
lockfile, and runs application, runtime, single-worker browser, and real
container lifecycle gates.
