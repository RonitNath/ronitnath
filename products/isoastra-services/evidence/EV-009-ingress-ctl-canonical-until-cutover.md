---
id: EV-009
date: 2026-08-06
provenance: ground-truth
source: owner ruling, 2026-08-06 (same day as the handoff): ingress-ctl centralized as system of truth, shadow controller runtime retired
---
Owner ruling (paraphrase of record; the ruling is documented in `resources/networks.md` and the
handoff): `ingress-ctl` + the nanode `service-ingress` daemon are the system of truth for
public-edge routes, and the old controller's shadow runtime was torn down 2026-08-06.

Consequence for this bet: the rebuild supersedes that state but does not contradict it — the live
daemon and its ~29 route files remain canonical until this product ships and cuts over. Nothing in
this bet touches the live nanode daemon, its routes, or DNS before that cutover. Migration posture
(strangler vs big cutover, activation gates) is a pitch question.
