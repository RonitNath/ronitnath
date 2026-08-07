---
id: EV-021
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"doesn't really touch cloudflare at all"

DNS is **out of scope**. This service manages per-edge routing only; it writes no Cloudflare
records, holds no Cloudflare credential, and expresses no DNS intent. The retired controller's
"declarative Cloudflare intent" half does not carry forward.

Consequence, and it is deliberate: the Cloudflare georouting/load-balancing the owner wants
(EV-013) is a separate, manually-configured concern. Multi-edge capability here means *every edge
can serve the route*; which edge a visitor reaches stays a DNS decision made outside this system.
The unmanaged-zone concept (EV-020) is therefore moot — there are no managed zones.
