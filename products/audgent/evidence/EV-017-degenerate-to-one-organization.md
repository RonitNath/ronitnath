---
id: EV-017
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (dictated)
---
"Honestly, we can just [degenerate] to one organization. There's never a case where we want to need or need tenancy because we're not selling voice agents as a platform. We're selling the products which live on top of that. So we have a front desk service, and that plugs downward into the voice agent platform. And so there's never the case of, like, what about another tenant who wants to use this? Because we will just be making another product, and that product will have its own authentication key against the voice agent platform instead of a customer themselves going to the voice agent platform and then building their own voice agent."

Resolves EV-005 and the rinity collision. audgent runs as **one organization**; the unit that
gets its own credential is a *product*, not a customer. rinity holds whatever per-customer
separation its own market requires, above this line. Tenancy is degenerated rather than
excised — "we can just degenerate to one organization" is an instruction to stop letting the
org concept drive the design, which is what unblocks whole-system observability (EV-010);
whether to also delete the columns is an engineering call bounded by EV-007's stopgap
appetite. Consequence: per-org scoping, quotas, and concurrency slots stop being product
surface; product-level auth keys become the caller model.
