---
id: EV-030
date: 2026-08-06
provenance: ground-truth
source: owner ruling on DEBATE-001 escalation E3 (claim C-29)
---
"yes" — in answer to: is there an audit sink outside this cluster?

**Yes.** Dangerous actions are exported to an append-only sink outside the services cluster, so the
primary record of what happened is not controlled by the service that might be the thing that failed
or was compromised. Taken as the recommended option (c): the **dangerous-action set**, not
everything — credential issuance and revocation, protected-host and wildcard route diffs, production
mail grants, edge applied-digest mismatches, and authentication denials. Ordinary reads and routine
single-route updates stay in-cluster.

This is load-bearing in a way it would not have been before two other rulings: EV-027 says the owner
is rarely watching, so an in-band alarm has no human behind it; and EV-029 makes the cluster trusted,
which means this sink is the **only** detector not controlled by the subject. It is the compensating
control for both.

Sharpened by C-44: a dashboard inside a failed quorum cannot be the alarm for its own failure, so
quorum loss belongs in the same external path.

The observability plane already runs on alien/tor, independent of nexus/sfo/nyc, so the destination
plausibly exists — whether it is the right sink for an append-only audit record is a design-gate
question, not settled here.
