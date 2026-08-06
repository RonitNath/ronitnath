---
id: EV-016
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"I'm thinking Nexus, sfo, and NYC as the hiqlite cluster. And this is the default kind of set of units. […] this set of servers should essentially never go down. If it goes down, that is extremely bad, and that is an emergency kind of instance. And we should be doing drills to ensure that this service is strong, and, also, we should be designing this well so it gracefully degrades, um, and it has no hard app failures, no unwraps or expects, uh, and is properly well written Rust code."

Cluster membership is ruled: **nexus, sfo, nyc** — the same three nodes as the universe reference
deployment. Availability posture: this is the fleet's most availability-critical service; its being
down is an emergency, because routing and mail for everything else depend on it.

Three engineering rules follow, and they are acceptance-shaped, not aspirational: (1) failover
drills prove the cluster, not green deploys (`procedures/ha-service.md`); (2) the design degrades
gracefully — partial failure must have a defined reduced mode, not an outage; (3) no `unwrap`/
`expect` panic paths in the Rust — a hard app failure is a design defect here.
