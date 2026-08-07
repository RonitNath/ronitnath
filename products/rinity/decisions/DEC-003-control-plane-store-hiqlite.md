# DEC-003 — Control-plane store: hiqlite

- date: 2026-08-06
- status: decided (owner, data gate ③ — EV-049 ruling 1)
- supersedes: the as-built plain-SQLite spine (EV-010/RF-03) for product data

Rinity's control-plane product data (persons, calls, grades, summaries, holds, config versions,
scrape tasks, run costs, audit, ladder state — data-model brief §1) lives in **hiqlite**, not
plain SQLite and not Postgres. The brief recommended SQLite-stays; the owner overrode to
hiqlite, aligning both portfolio products on the same store (pilot DEC-013).

Rules carried from DEC-013:
- **Additive-only migrations** for the bet's duration; never roll back a migration after call
  records have been ingested — restore from backup is the rollback path.
- **Backup must exist before the first product write**; restore drill recorded before the first
  stranger practice.
- hiqlite's **notify bus** substitutes for LISTEN/NOTIFY where cross-process fan-out is needed
  (pilot note: needs the `cache` feature); single-node in-process SSE fan-out needs neither.
- Single node now; the Raft path is why hiqlite rather than plain SQLite — multi-node comes
  without a store migration.

PHI note from the brief stands: the store becomes PHI-bearing at the fake-patient → real
transition and the D2-4 classification extends to it. With EV-049 ruling 5 (indefinite media
retention) the store's PHI posture is permanent once real patients arrive.

Consequence for the existing schema: the one as-built migration (tenancy spine) carries over;
the web_template SQLite/SQLx wiring is replaced by hiqlite client wiring at build time — a
workorder concern, cited against this decision.
