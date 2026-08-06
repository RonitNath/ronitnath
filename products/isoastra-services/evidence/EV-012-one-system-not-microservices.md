---
id: EV-012
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"previously, I want to split these services out, um, and so they would not be conflicting with each other. And if one of them went down, that didn't bring everything down. And so I tend going towards a microservice architecture, but I realized that this is going to end up leading to being very fragmented because I've emailing, I'll have SMS later. I have, uh, ingress right now, perhaps firewall stuff, uh, deployment configuration, then there's speech to text, text to speech, large language models, embedding. There's also internal versus external services, meaning things services for hosted online and hardware. And all this is just going to be a mess. […] in the process of, like, building all these services, which can be self healing and highly available and whatever or not, I realized that I'm essentially replicating the same kind of process a bunch of times and leading myself to having twenty different services which are trying to perform the same work. But, like, I need to manage all these processes individually instead of just as one system. And so there's more coupling, which adds risk. But at the same time, everything is together, so I don't have to think about as many of the components. And as someone working as a sole founder, I think having fewer components is a better methodology for me to follow."

The founding thesis, and it reverses a prior direction. Microservice fragmentation is the failure
being avoided, not the goal: each split-out service re-pays the same HA/self-healing/deployment tax
and must be operated individually. The trade is named and accepted — more coupling (more risk) in
exchange for fewer components to hold in one founder's head.

Enumerated future service domains (the space the architecture must leave room for, not build):
mailing, SMS, ingress/routing, firewall, deployment configuration, speech-to-text, text-to-speech,
LLMs, embeddings — and a cross-cutting split between internal vs external and hosted vs hardware.
Insight source: the universe-ronitnath rebuild.
