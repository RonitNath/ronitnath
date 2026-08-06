---
id: EV-004
date: 2026-08-06
provenance: ground-truth
source: owner, bet-opening session
---
"Right now, only me (ronit) and agents use this, but later employees will have narrow access. Let's not add this role now, but we should have RBAC."

Two rulings in one sentence: (1) authorization is role-based from day one — access is expressed
through roles even while the only principals are the owner and agents; (2) the employee role itself
is explicitly deferred — do not design or ship it now, but nothing may assume "all authenticated
principals are omnipotent" in a way that makes narrow roles a rewrite later.
