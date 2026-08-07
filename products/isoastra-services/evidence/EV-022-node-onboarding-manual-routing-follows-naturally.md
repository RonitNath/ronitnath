---
id: EV-022
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"no, just manage routing, new nodes will be a manual process right now, but when new nodes are added, it should be pretty natural for this service to manage routing for them"

Node onboarding automation is **out of T1**. Bringing a new public edge online stays a manual
(Ansible/fleet) process. What is in scope is the shape that makes it cheap afterwards: adding an
edge to this service must be a small, obvious act — registering the edge, after which every
existing route applies to it without per-route work. "Natural" is the acceptance bar; the design
fails if adopting a new edge means revisiting routes one at a time.
