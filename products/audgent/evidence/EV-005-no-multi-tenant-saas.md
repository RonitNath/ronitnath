---
id: EV-005
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM-mode kickoff message 2026-08-06
---
"Multi-tenant saas isn't what I need from it."

Retires multi-tenancy as an audgent requirement. Today tenancy is load-bearing throughout:
organizations are the scoping unit for workflows, telephony configs, phone numbers,
credentials, model configuration, service principals, and runs; per-org concurrency slots and
quota checks gate admission; the caller seam resolves tenant-scoped scopes
(EV-011/AC-11). Consequence: either that machinery is removed (a deletion bet through the
data layer, touching nearly every table) or it is demoted to a degenerate single-org case and
stops driving the design. Directly reopens a cross-product question — rinity is external and
sells to multiple customers (rinity/EV-015), so if audgent stops being multi-tenant, tenancy
has to live in rinity's plane or in per-deployment isolation. That is an interview topic, not
a settled consequence.
