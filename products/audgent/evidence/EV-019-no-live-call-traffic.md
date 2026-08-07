---
id: EV-019
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (constraints topic)
---
"Nothing's carrying calls. Calls shouldn't be affected anyways."

Removes the constraint the whole bet appeared to be shaped around. No telephony provider is
carrying live traffic today, so "the production surface" (EV-007) means *the surface
production will run on*, not a system with calls in flight to protect. Two consequences, and
the second is the one that matters: (a) no provider is untouchable, no migration needs a
zero-downtime story, and there is no traffic to break; (b) the planned work — tenancy
degeneration, observability, the agent surface — doesn't sit in the call path anyway, so
call-path risk is not an argument against any of it. This is what makes a schema-level
tenancy teardown (EV-017) affordable rather than reckless.
