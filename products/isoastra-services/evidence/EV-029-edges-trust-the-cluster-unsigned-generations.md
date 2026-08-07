---
id: EV-029
date: 2026-08-06
provenance: ground-truth
source: owner ruling on DEBATE-001 escalation E2 (claim C-27)
---
"trust" — in answer to: do edges verify a signature on each config generation, or trust the cluster
that renders it?

**Edges trust the cluster. Generations are unsigned.** No external signer, no pinned verification
key at the edge, no owner step-up gate on protected hostnames as a cryptographic control. The
smallness argument (C-46/C-48's mandate — another key and another signer is another thing broken at
3am) wins over the cluster-compromise argument.

What was given up knowingly, stated plainly because the lens made it concrete: **compromise of any
one cluster node can repoint every hostname at the next poll**, while that same node can falsify its
own audit trail and status views. DNS is untouched, so users still reach familiar TLS hostnames.
The services cluster is now, explicitly, as privileged as the routes it serves.

Two consequences that do *not* follow, and must not be quietly assumed:

- This does not remove the need for **capability scoping** (C-26, C-36). Trusting the cluster is not
  trusting every credential the cluster issues; an agent credential still may not touch wildcards,
  apexes, catch-alls, raw config or edge adoption.
- It raises the value of EV-030's external audit sink, which becomes the only detector not controlled
  by the thing it would be detecting.

**What would change this**: a second human principal with routing authority, or any external party
gaining a credential to this cluster. The trust model rests on the cluster's principals being the
owner and his agents.
