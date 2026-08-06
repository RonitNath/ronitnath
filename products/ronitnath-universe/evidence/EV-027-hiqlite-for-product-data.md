---
id: EV-027
date: 2026-08-06
provenance: ground-truth
source: owner, data gate (session ③)
supersedes: PITCH-001 §1's "PostgreSQL seam", PKT-01's constraint, DEC-006's LISTEN/NOTIFY candidate
---
"1. hiqlite."

Answering the data gate's first escalation: PITCH-001 §1, PKT-01 and DEC-006 all name a PostgreSQL
seam, while the deployed reality is hiqlite by an owner override made the same day — an override
issued when the store held exactly one disposable table (`node_release`) and never weighed against
the pitch's own words.

**The override extends to product data.** Identity, responses, attendance, links and copy all live in
the embedded Raft-replicated SQLite already running on sfo, nyc and nexus. Formalized as DEC-013.

`monorepo.md`'s PostgreSQL/Patroni ruling is untouched everywhere else; this stays scoped to
universe-ronitnath, as the original override was.
