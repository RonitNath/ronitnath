---
id: EV-032
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-2)
---
"sqlite"

Resolves OQ-2. SQLite, not the PostgreSQL seam ruled for universe-ronitnath and not the
hiqlite chosen for isoastra-services. Fits the posture: single node, internal use only
(EV-013), testing mode (EV-006), and no HA obligation. Consequence: the data layer is a
local file, so the whole system is trivially resettable and copyable — a corpus plus its
database is a portable artifact — and no database server is a prerequisite for rung 1.
Carries a graduation cost to be paid later, since a production Isoastra deployment will
likely want Postgres; keeping plain SQL close to the surface (the universe doctrine, no ORM
magic) is what keeps that translation cheap.
