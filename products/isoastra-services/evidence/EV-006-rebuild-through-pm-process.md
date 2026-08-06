---
id: EV-006
date: 2026-08-06
provenance: ground-truth
source: owner, ingress-rebuild commissioning session (quoted in the 2026-08-06 handoff document)
---
"I want to entirely rebuild the controller project through the new portfolio PM process."

The routing half of this product is a rebuild, not a rescue: the retired shadow controller in
`Isoastra/service-ingress` (catalog, per-edge Caddyfile compilers, Cloudflare intent, PG-advisory-lock
peer lease) is prior art and evidence only. Whether anything is reused is a pitch/design decision,
never an assumption — the PG-lease design solved coordination that hiqlite now owns.
