---
id: EV-020
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"Digital Ocean is on a compliance posture. So […] I can handle PHI traffic on that, but I can't handle it on other nodes."

and separately:

"DentConnex dot com is going to soon transfer onto my own domain control and off of Wix. And so that's not as relevant."

Two rulings about what routing must know. (1) **Data class is a real routing constraint**: PHI
traffic may traverse only DigitalOcean nodes (the BAA provider, per `procedures/deployment.md`), so
the route model carries a data class and placement cannot be edge-agnostic — the old catalog's
`standard`/`phi` policy model was solving a real problem. (2) The **dentconnex.com unmanaged-zone
carve-out is expiring**: the zone moves from client-owned Wix DNS to the owner's control, so the
"explicitly unmanaged zone" fence designed around it stops being the motivating case (a general
unmanaged-zone concept may still be warranted — that is a design question, not a ruling).
