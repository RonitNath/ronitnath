---
id: EV-002
date: 2026-08-06
provenance: ground-truth
source: owner, bet-opening session
---
"It'll use the same tech stack as the universe-ronitnath rebuild."

The reference stack is ruled, not open: Rust + Leptos (islands mode) + Axum + hiqlite, deployed as
one OCI image per node with byte-identical compose and per-node `node.env`
(`~/dev/context/procedures/ha-service.md`; reference implementation `~/dev/personal/universe-ronitnath`,
fleet role `universe_deploy`). The hard-won hiqlite lessons carry: bootstrap is a different mode from
steady state, additive-only migrations, immutable applied migration files, backup before first
product write, failover drill as acceptance (ronitnath-universe DEC-013).
