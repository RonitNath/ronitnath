---
id: EV-018
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
supersedes: none — refines EV-009
---
"This service and the previous service are both going to coexist. We'll just keep them both active. And once I've finished the full universe rewrite, that will be when the old service goes down. But there is no current timeline for when the old service should go down. It's just that once we're ready with ingress ctl, we'll also just leave ingress ctl to the side and then use this new service to do routing. And if we ever have any issues, ingress ctl is still available for us."

Migration posture is settled and it is the cheap one: **coexistence, not cutover**. Both mail
services stay active; the old mailer is retired only after the universe rewrite completes, with no
timeline. ingress-ctl is "left to the side" — kept working and available as the fallback if the new
routing surface has problems, and decommissioned later (EV-013 notes the owner also expects
eventual decommission; the ruling here is that it is not gated on this bet).

Consequence: no big-bang activation gate is needed, but **both writers must not fight**. Two systems
able to write the same edge config or send as the same identity is the risk this posture creates,
and the design owns it.
