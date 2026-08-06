---
id: EV-013
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"ingress ctl is not great right now because it only works on Nanode and is pointing to Nanode. And this makes Nanode a single point of failure for routing. Eventually, I want routing to be able to go to any of my ingress nodes, and, also, I'll spin up more nodes in order to have public ingress. […] At the same time, I want to set up Cloudflare georouting and load balancing. So DNS is balanced against all public servers, so I don't have a single point of failure on that node. However, as I keep pushing products out, I just have AI agents, and I talk to them and I say, deploy this product, and they figure out the rest. And part of this figuring out is using the service to update the public routes. So the routes will go to wherever the application is appropriately deployed. I don't want to hand maintain all those across these different machines. Further, if I'm doing elastic scaling and adding new nodes in or increasing the size of nodes, I want to be able to just reuse the Ansible playbook to be able to have new nodes come online and not have to think about setting up the routing for them individually and being able to do all the networking checks between different machines."

(Transcription: "ingress ctl" appears as "English cuddle" in the raw voice transcript.)

What routing must become, in four parts: (1) it is nanode-independent — the control surface neither
runs on nanode nor points only at it; (2) Cloudflare georouting + load balancing spreads DNS across
all public edges so no single edge is a SPOF; (3) the primary caller is an **agent** deploying a
product — "deploy this" must include registering the route to wherever the app landed, with no
hand-maintenance across machines; (4) node onboarding via the existing Ansible playbook must bring
routing and inter-machine networking checks along, not require per-node routing setup.

Also noted by the owner: ingress-ctl recently gained HA (multi-upstream) route support, which is new
to the ingress surface and predates this rebuild.
