---
id: EV-028
date: 2026-08-06
provenance: ground-truth
source: owner ruling on DEBATE-001 escalation E1 (claim C-21)
---
"yes, keeps it" — in answer to: does the old mailer keep its own SES credential during indefinite
coexistence, or is it turned into a facade that enqueues into the new service?

**Dual SES custody is accepted for the coexistence period.** The live mailer keeps its own SES
credential and is not modified, which holds EV-024 ("don't touch other services") and EV-018's
indefinite fallback intact. C1's remedy — facading the old mailer under a legacy principal — is
rejected because it requires touching it.

Costs accepted knowingly, and they are the ones the lens named: SES credential rotation is a
coordinated two-system operation for as long as both run; no single principal ledger explains a
provider-side event across both; and the "one place credentials live" property (EV-014) is a
property of the *new* service, not yet of the fleet.

This does not weaken C-22 or C-23 inside the new service: its own SES access still uses short-lived
sessions from a vault, and an SES secret still may not live in hiqlite.
