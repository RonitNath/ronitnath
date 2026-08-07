---
id: DEC-001
date: 2026-08-06
source: owner ruling EV-030, on the source check EV-029 §3, resolving BRIEF-001/OQ-1
---
Chose **degenerating tenancy to a single organization** over **removing org scoping from the
schema**, because removal is 41 model columns, 202 files and 98 migrations of mechanical
change (EV-029) bought for a benefit — a smaller schema — that no flow in FLOWS-001 depends
on. Degeneration delivers the entire owner-visible outcome of EV-017 and EV-010 without a
migration.

| Rule | What the build must now do |
| --- | --- |
| One org, forever | Exactly one organization row exists. Nothing creates a second. Its id is a constant, not a parameter |
| Reads never scope | Every owner-facing read (F-1..F-5, F-10) queries the whole system. A view that filters by org is a bug, not a configuration — this is the entire point of EV-010 |
| No org in the interface | No org switcher, no org selector, no org creation or invitation path, no org name displayed as context. The concept is invisible above the data layer |
| Keys identify products, not tenants | Product-level credentials (rinity's, agents') resolve to the single org implicitly. A caller never names an org, and never can (EV-017) |
| Limits become global | Per-org concurrency slots and quota checks become system limits. They keep working; they stop meaning "per tenant" |
| No new org branching | New code may read the constant but must not add org-conditional behavior. Existing per-org routing (e.g. Langfuse exporters) is left in place, unused and unfed |
| Columns stay | `organization_id` columns are not dropped, not renamed, not migrated. They are load-bearing plumbing carrying a constant |

**What was given up knowingly**: a genuinely smaller schema and the absence of a vestigial
concept. Removal would make it *impossible* for org scoping to creep back into a query and
quietly hide data from the owner — which is exactly the defect this bet exists to fix
(EV-010). Degeneration leaves that failure mode reachable, and it will not announce itself:
a scoped query returns plausible results, just incomplete ones. EV-023's long horizon (this
codebase is around for a while) is the strongest argument for having paid the removal cost.

**What would change this**: repeated scoping bugs — a view found filtering by org, or data
found missing from a whole-system read — would mean the vestigial concept is not inert and
removal should be reconsidered. So would any need for genuine isolation between products,
which EV-017 rules out but a second consumer could reopen (EV-009).
