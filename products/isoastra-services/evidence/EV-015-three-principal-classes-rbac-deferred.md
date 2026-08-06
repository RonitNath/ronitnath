---
id: EV-015
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
supersedes: none — refines EV-004
---
"role based access control is not going to apply to anyone in the near term. Right now, I'm the only person who needs to use these services, and the second person is the agents who are going to be doing the configuration. And if they're doing any debugging or forensics, and third is going to be my applications which plug into this services app in order to have access to external third party services. Agents can use Kanidm. I mean, honestly, they should just have an easy way to use the application. And the Kanidm path should potentially be streamlined. Honestly, once the universe rewrite properly lands, we probably will not still be using Kanidm anyways. So I guess that's just a seam that we should be wary of."

(Transcription: "Kanidm" appears as "con IBM"/"con IDM" in the raw voice transcript.)

Refines EV-004. Three principal classes, not one: **owner** (human), **agents** (configuration,
debugging, forensics), **applications** (consumers plugging in for third-party service access — the
mail senders today, gateway consumers later). RBAC differentiates *no one* in the near term, so the
role model must exist without any near-term role content to justify it.

Identity is a flagged seam: agents authenticate via Kanidm today and the path should be streamlined,
but Kanidm itself is expected to be replaced once the universe rewrite lands. The design must not
weld itself to Kanidm.
