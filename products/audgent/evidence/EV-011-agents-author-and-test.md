---
id: EV-011
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (dictated)
---
"the main thing is agents need to be able to create workflows and test workflows and also be able to do configuration for providers and test providers."

Enumerates the agent capability set, and resolves EV-006's ambiguity: "agents" means the
owner's worker agents operating audgent, not the voice agents it runs. Four verbs —
**create workflow, test workflow, configure provider, test provider** — and the two *test*
verbs are the load-bearing half, because they are what an agent needs to close its own loop
without a human confirming that a change worked. Consequence: authoring APIs alone are
insufficient; the agent-facing surface must include a way to exercise a workflow or a
provider and read back a verdict. Ties to EV-013 — that testing traffic must be
distinguishable from production.
