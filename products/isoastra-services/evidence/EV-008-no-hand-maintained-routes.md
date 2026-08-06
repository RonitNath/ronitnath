---
id: EV-008
date: 2026-08-06
provenance: ground-truth
source: owner, ingress-rebuild commissioning session (quoted in the 2026-08-06 handoff document)
---
"I don't want to hand-maintain routes across all the public ingress nodes, including if i bring up another linode node."

The problem statement for routing: adding a second/third public edge must never mean hand-editing
route config per machine. Desired-state route intent, applied to every edge, is the point of the
routing domain.
