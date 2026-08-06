---
id: EV-007
date: 2026-08-06
provenance: ground-truth
source: owner, ingress-rebuild commissioning session (quoted in the 2026-08-06 handoff document)
---
"Key changes are to use hiqlite, have an interface for me, and yes on not being on nanode itself. I'm preparing for a world where nanode is no longer a SPOF."

Three rulings: hiqlite replaces the old PostgreSQL-advisory-lock coordination; there is an
owner-facing interface (an operator UI, not just a CLI); and the control plane does not live on
nanode. The design premise is multiple public edges with nanode as one edge among several — never
a single point of failure the control plane depends on.
