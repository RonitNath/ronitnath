---
id: EV-010
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (dictated)
---
"I want observability onto everything and everything which is going on. But the interface right now is built so I just have access to my own workspace and my own workflows. And this is not really what I'm looking for out of this project."

The core defect of the inherited design, stated as a want: the owner needs to see the whole
system, and a multi-tenant console structurally can't show it — it shows *a tenant* their own
slice by construction, and the owner is just another tenant inside it. This is the same
complaint the owner has about Stalwart's admin surface. Consequence: the fix is not "add a
dashboard"; the scoping model itself is what hides the system (see EV-005, EV-017). Whole-
system visibility is the acceptance test for the human surface — if a call, a config change,
or a cost exists, the owner can see it without switching context.
